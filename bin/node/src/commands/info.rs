//! Info Subcommand

use crate::flags::GlobalArgs;
use clap::Parser;
use kona_registry::{OPCHAINS, ROLLUP_CONFIGS};
use tracing::info;

/// The `info` Subcommand
///
/// The `info` subcommand is used to run the information stack for the `kona-node`.
///
/// # Usage
///
/// ```sh
/// kona-node info
/// ```

#[derive(Parser, Default, PartialEq, Debug, Clone)]
#[command(about = "Runs the information stack for the kona-node.")]
pub struct InfoCommand;

impl InfoCommand {
    /// Initializes the logging system based on global arguments.
    pub fn init_logs(&self, args: &GlobalArgs) -> anyhow::Result<()> {
        args.init_tracing(None)?;
        Ok(())
    }

    /// Runs the information stack for the kona-node.
    pub fn run(&self, args: &GlobalArgs) -> anyhow::Result<()> {
        info!("Running info command");

        let op_chain_config = OPCHAINS.get(&args.l2_chain_id).expect("No Chain config found");
        let op_rollup_config =
            ROLLUP_CONFIGS.get(&args.l2_chain_id).expect("No Rollup config found");

        println!("Name: {}", op_chain_config.name);
        println!("Block Time: {}", op_chain_config.block_time);
        println!("Identifier: {}", op_chain_config.chain_id);
        println!("Public RPC - {}", op_chain_config.public_rpc);
        println!("Sequencer RPC - {}", op_chain_config.sequencer_rpc);
        println!("Explorer - {}", op_chain_config.explorer);
        println!("Hardforks: {}", op_rollup_config.hardforks);
        println!("-------------");

        Ok(())
    }
}
