//! Discovery Module.

use derive_more::Debug;
use discv5::{Discv5, Enr, Event, enr::NodeId};
use libp2p::Multiaddr;
use std::path::PathBuf;
use tokio::{
    sync::mpsc::channel,
    time::{Duration, sleep},
};

use crate::{
    BootNode, BootNodes, BootStore, Discv5Builder, Discv5Handler, EnrValidation, HandlerRequest,
    HandlerResponse, OpStackEnr, enr_to_multiaddr,
};

/// The [`Discv5Driver`] drives the discovery service.
///
/// Calling [`Discv5Driver::start`] spawns a new [`Discv5`]
/// discovery service in a new tokio task and returns a
/// [`Discv5Handler`].
///
/// Channels are used to communicate between the [`Discv5Handler`]
/// and the spawned task containing the [`Discv5`] service.
///
/// Since some requested operations are asynchronous, this pattern of message
/// passing is used as opposed to wrapping the [`Discv5`] in an `Arc<Mutex<>>`.
/// If an `Arc<Mutex<>>` were used, a lock held across the operation's future
/// would be needed since some asynchronous operations require a mutable
/// reference to the [`Discv5`] service.
///
/// ## Example
///
/// ```no_run
/// use kona_p2p::Discv5Driver;
/// use std::net::{IpAddr, Ipv4Addr, SocketAddr};
///
/// #[tokio::main]
/// async fn main() {
///     let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9099);
///     let mut disc = Discv5Driver::builder()
///         .with_address(socket)
///         .with_chain_id(10) // OP Mainnet chain id
///         .build()
///         .expect("Failed to build discovery service");
///     let mut handler = disc.start();
///
///     loop {
///         if let Some(enr) = handler.enr_receiver.recv().await {
///             println!("Received peer enr: {:?}", enr);
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Discv5Driver {
    /// The [`Discv5`] discovery service.
    #[debug(skip)]
    pub disc: Discv5,
    /// The [`BootStore`].
    pub store: BootStore,
    /// The chain ID of the network.
    pub chain_id: u64,
    ///
    /// The interval to discovery random nodes.
    pub interval: Duration,
}

impl Discv5Driver {
    /// Returns a new [`Discv5Builder`] instance.
    pub fn builder() -> Discv5Builder {
        Discv5Builder::new()
    }

    /// Instantiates a new [`Discv5Driver`].
    pub fn new(
        disc: Discv5,
        interval: Duration,
        chain_id: u64,
        bootstore: Option<PathBuf>,
    ) -> Self {
        let store = BootStore::from_chain_id(chain_id, bootstore);
        Self { disc, chain_id, store, interval }
    }

    /// Starts the inner [`Discv5`] service.
    async fn init(&mut self) {
        loop {
            if let Err(e) = self.disc.start().await {
                warn!("Failed to start discovery service: {:?}", e);
                sleep(Duration::from_secs(2)).await;
                info!("Retrying discovery startup...");
                continue;
            }
            break;
        }
    }

    /// Adds a bootnode to the discv5 service given an enode address.
    async fn add_enode(&mut self, enode: Multiaddr) -> Option<Enr> {
        let enr = match self.disc.request_enr(enode.clone()).await {
            Ok(enr) => enr,
            Err(err) => {
                error!(
                    ?enode,
                    %err,
                    "failed to add boot node"
                );

                return None;
            }
        };

        if let Err(err) = self.disc.add_enr(enr.clone()) {
            warn!(
                ?enode,
                %err,
                "failed to add boot node"
            );

            return None;
        }

        Some(enr)
    }

    /// Bootstraps the [`Discv5`] service with the bootnodes.
    async fn bootstrap(&mut self) {
        let nodes = BootNodes::from_chain_id(self.chain_id);

        let boot_enrs: Vec<Enr> = nodes
            .0
            .iter()
            .filter_map(|bn| match bn {
                BootNode::Enr(enr) => Some(enr.clone()),
                _ => None,
            })
            .collect();

        // First attempt to add the bootnodes to the discovery table.
        let mut count = 0;
        for enr in &boot_enrs {
            let validation = EnrValidation::validate(enr, self.chain_id);
            if validation.is_invalid() {
                debug!(target: "discovery", "Ignoring Invalid Bootnode ENR: {:?}. {:?}", enr, validation);
                continue;
            }
            match self.disc.add_enr(enr.clone()) {
                Ok(_) => count += 1,
                Err(e) => debug!(target: "discovery", "[BOOTSTRAP] Failed to add enr: {:?}", e),
            }
        }
        info!(target: "discovery", "Added {} Bootnode ENRs to discovery table", count);

        // Add ENRs in the bootstore to the discovery table.
        //
        // Note, discv5's table may not accept new ENRs above a certain limit.
        // Instead of erroring, we log the failure as a debug log.
        count = 0;
        for enr in self.store.valid_peers() {
            let validation = EnrValidation::validate(enr, self.chain_id);
            if validation.is_invalid() {
                debug!(target: "discovery", "Ignoring Invalid Bootnode ENR: {:?}. {:?}", enr, validation);
                continue;
            }
            match self.disc.add_enr(enr.clone()) {
                Ok(_) => count += 1,
                Err(e) => debug!(target: "discovery", "Failed to add ENR to discv5 table: {:?}", e),
            }
        }
        info!(target: "discovery", "Added {} Bootstore ENRs to discovery table", count);

        // Merge the bootnodes into the bootstore.
        self.store.merge(boot_enrs);
        debug!(target: "discovery",
            new=%count,
            total=%self.store.len(),
            "Added new ENRs to discv5 bootstore"
        );

        let mut boot_enodes_enrs = Vec::new();
        for node in nodes.0 {
            match node {
                BootNode::Enode(enode) => {
                    let Some(enr) = self.add_enode(enode).await else {
                        continue;
                    };

                    boot_enodes_enrs.push(enr);
                }
                BootNode::Enr(_) => { /* ignore: bootnode enrs already added */ }
            }
        }
        info!(target: "discovery", "Added {} Bootnode ENODEs to discovery table", boot_enodes_enrs.len());

        // Merge the bootnodes into the bootstore.
        self.store.merge(boot_enodes_enrs);
        debug!(target: "discovery",
            new=%count,
            total=%self.store.len(),
            "Added new ENRs to discv5 bootstore"
        );
    }

    /// Sends ENRs from the boot store to the enr receiver.
    pub async fn forward(&mut self, enr_sender: tokio::sync::mpsc::Sender<Enr>) {
        for enr in self.store.peers() {
            if let Err(e) = enr_sender.send(enr.clone()).await {
                info!(target: "discovery", "Failed to forward enr: {:?}", e);
            }
        }
    }

    /// Spawns a new [`Discv5`] discovery service in a new tokio task.
    ///
    /// Returns a [`Discv5Handler`] to communicate with the spawned task.
    pub fn start(mut self) -> Discv5Handler {
        let chain_id = self.chain_id;
        let (req_sender, mut req_recv) = channel::<HandlerRequest>(1024);
        let (res_sender, res_recv) = channel::<HandlerResponse>(1024);
        let (enr_sender, enr_recv) = channel::<Enr>(1024);
        let (_events_sender, events_recv) = channel::<Event>(1024);

        tokio::spawn(async move {
            // Step 1: Start the discovery service.
            self.init().await;
            info!(target: "discovery", "Started Discv5 Peer Discovery");

            // Step 2: Bootstrap discovery service bootnodes.
            self.bootstrap().await;
            let enrs = self.disc.table_entries_enr();
            info!(target: "discovery", "Bootstrapped Discv5 Enr Table with {} ENRs", enrs.len());

            // Step 3: Forward ENRs in the bootstore to the enr receiver.
            self.forward(enr_sender.clone()).await;

            // Interval to find new nodes.
            let mut interval = tokio::time::interval(self.interval);

            // Interval at which to sync the boot store.
            let mut store_interval = tokio::time::interval(Duration::from_secs(60));

            // Step 3: Run the core driver loop.
            loop {
                tokio::select! {
                    msg = req_recv.recv() => {
                        match msg {
                            Some(msg) => match msg {
                                HandlerRequest::Metrics => {
                                    let metrics = self.disc.metrics();
                                    let _ = res_sender.send(HandlerResponse::Metrics(metrics)).await;
                                }
                                HandlerRequest::PeerCount => {
                                    let peers = self.disc.connected_peers();
                                    let _ = res_sender.send(HandlerResponse::PeerCount(peers)).await;
                                }
                                HandlerRequest::LocalEnr => {
                                    let enr = self.disc.local_enr().clone();
                                    let _ = res_sender.send(HandlerResponse::LocalEnr(enr)).await;
                                }
                                HandlerRequest::AddEnr(enr) => {
                                    let _ = self.disc.add_enr(enr);
                                }
                                HandlerRequest::RequestEnr(enode) => {
                                    let _ = self.disc.request_enr(enode).await;
                                }
                                HandlerRequest::TableEnrs => {
                                    let enrs = self.disc.table_entries_enr();
                                    let _ = res_sender.send(HandlerResponse::TableEnrs(enrs)).await;
                                },
                                HandlerRequest::BanAddrs{addrs_to_ban, ban_duration} => {
                                    let enrs = self.disc.table_entries_enr();

                                    for enr in enrs {
                                        let Some(multi_addr) = enr_to_multiaddr(&enr) else {
                                            continue;
                                        };

                                        if addrs_to_ban.contains(&multi_addr) {
                                            self.disc.ban_node(&enr.node_id(), Some(ban_duration));
                                        }
                                    }
                                }
                            }
                            None => {
                                trace!(target: "discovery", "Receiver `None` peer enr");
                            }
                        }
                    }
                    _ = interval.tick() => {
                        match self.disc.find_node(NodeId::random()).await {
                            Ok(nodes) => {
                                let enrs =
                                    nodes.into_iter().filter(|node| OpStackEnr::is_valid_node(node, self.chain_id));

                                for enr in enrs {
                                    self.store.add_enr(enr.clone());
                                    _ = enr_sender.send(enr).await;
                                }
                            }
                            Err(err) => {
                                debug!(target: "discovery", "Failed to find node: {:?}", err);
                            }
                        }
                    }
                    _ = store_interval.tick() => {
                        let enrs = self.disc.table_entries_enr();
                        self.store.merge(enrs);
                        self.store.sync();
                    }
                }
            }
        });

        Discv5Handler::new(chain_id, req_sender, res_recv, events_recv, enr_recv)
    }
}

#[cfg(test)]
mod tests {
    use crate::BootNodes;
    use discv5::{enr::CombinedPublicKey, handler::NodeContact};
    use kona_genesis::OP_SEPOLIA_CHAIN_ID;

    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_discv5_driver() {
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9099);
        let discovery = Discv5Driver::builder()
            .with_address(socket)
            .with_chain_id(OP_SEPOLIA_CHAIN_ID)
            .build()
            .expect("Failed to build discovery service");
        let handle = discovery.start();
        assert_eq!(handle.chain_id, OP_SEPOLIA_CHAIN_ID);
    }

    #[tokio::test]
    async fn test_discv5_driver_bootstrap_testnet() {
        // Use a test directory to make sure bootstore
        // doesn't conflict with a local bootstore.
        let dir = std::env::temp_dir();
        assert!(std::env::set_current_dir(&dir).is_ok());

        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9099);
        let mut discovery = Discv5Driver::builder()
            .with_address(socket)
            .with_chain_id(OP_SEPOLIA_CHAIN_ID)
            .build()
            .expect("Failed to build discovery service");
        discovery.store.path = dir.join("bootstore.json");

        discovery.init().await;

        discovery.bootstrap().await;

        // Verify bootstore has entries.
        assert!(!discovery.store.is_empty(), "No nodes were added to the bootstore");
        // Verify discv5 has entries in its enr table.
        assert!(!discovery.disc.table_entries_enr().is_empty());

        // It should have the same number of entries as the testnet table.
        let testnet = BootNodes::testnet();

        // Filter out testnet ENRs that are not valid.
        let testnet: Vec<CombinedPublicKey> = testnet
            .iter()
            .filter_map(|node| {
                if let BootNode::Enr(enr) = node {
                    // Check that the ENR is valid for the testnet.
                    if !OpStackEnr::is_valid_node(enr, OP_SEPOLIA_CHAIN_ID) {
                        return None;
                    }
                }
                let node_contact =
                    NodeContact::try_from_multiaddr(node.to_multiaddr().unwrap()).unwrap();

                Some(node_contact.public_key())
            })
            .collect();

        let disc_enrs = discovery.disc.table_entries_enr();
        for public_key in testnet {
            assert!(
                disc_enrs.iter().any(|enr| enr.public_key() == public_key),
                "Discovery table does not contain testnet ENR: {:?}",
                public_key
            );
        }
    }
}
