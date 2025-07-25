#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/op-rs/kona/main/assets/square.png",
    html_favicon_url = "https://raw.githubusercontent.com/op-rs/kona/main/assets/favicon.ico",
    issue_tracker_base_url = "https://github.com/op-rs/kona/issues/"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

mod config;
pub use config::RpcConfig;

mod launcher;
pub use launcher::{HealthzResponse, RpcLauncher, RpcLauncherError};

mod net;
pub use net::NetworkRpc;

mod p2p;

mod response;
pub use response::SafeHeadResponse;

mod output;
pub use output::OutputResponse;

mod jsonrpsee;
pub use jsonrpsee::{
    MinerApiExtServer, OpAdminApiServer, OpP2PApiServer, RollupNodeApiServer, WsServer,
};

#[cfg(feature = "reqwest")]
pub mod reqwest;
#[cfg(feature = "reqwest")]
pub use reqwest::SupervisorClient;

#[cfg(feature = "client")]
mod interop;
#[cfg(feature = "client")]
pub use interop::{CheckAccessListClient, InteropTxValidator, InteropTxValidatorError};

#[cfg(feature = "client")]
pub use kona_supervisor_rpc::SupervisorApiClient;

mod rollup;
pub use rollup::RollupRpc;

mod l1_watcher;
pub use l1_watcher::{L1State, L1WatcherQueries, L1WatcherQuerySender};

mod ws;
pub use ws::WsRPC;
