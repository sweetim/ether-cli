use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use clap::Parser;
use dotenv::dotenv;
use eyre::Result;
use prettytable::{format, table};
use serde::{Deserialize, Serialize};
use std::{env, str::FromStr};

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = env!("CARGO_PKG_DESCRIPTION"), long_about = None)]
struct CliOptions {
    /// input file for CLI options
    #[arg(short, long)]
    file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppOptions {
    wallet_private_key: String,
    to_address: String,
    amount_wei: u64,
    chain_id: SupportedChain,
}

impl AppOptions {
    fn get_from_file(path: String) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let options: AppOptions = serde_json::from_str(&contents)?;
        Ok(options)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SupportedChain {
    #[serde(rename = "sepolia")]
    Sepolia,
    #[serde(rename = "mainnet")]
    Mainnet,
}

impl SupportedChain {
    fn rpc_url(&self) -> String {
        match self {
            SupportedChain::Sepolia => env::var("SEPOLIA_RPC_URL").unwrap(),
            SupportedChain::Mainnet => env::var("MAINNET_RPC_URL").unwrap(),
        }
    }
}

impl FromStr for SupportedChain {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sepolia" => Ok(SupportedChain::Sepolia),
            "mainnet" => Ok(SupportedChain::Mainnet),
            _ => Err(eyre::eyre!("unsupported chain")),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let cli_options = CliOptions::parse();

    let cli_options_file_path = cli_options
        .file
        .expect("cli options json file should be included");

    let cli_options = AppOptions::get_from_file(cli_options_file_path.to_string())
        .expect("cli options json file schema incorrect, please refer to schema/cli-schema.json");

    let rpc_url = match cli_options.chain_id {
        SupportedChain::Sepolia => SupportedChain::Sepolia.rpc_url().parse()?,
        SupportedChain::Mainnet => SupportedChain::Mainnet.rpc_url().parse()?,
    };

    let to_wallet_address = Address::from_str(cli_options.to_address.as_str())?;

    let private_key: PrivateKeySigner =
        PrivateKeySigner::from_str(&cli_options.wallet_private_key)?;
    let owner_wallet_address = private_key.address();
    let owner_wallet = EthereumWallet::from(private_key);

    let provider = ProviderBuilder::new().wallet(owner_wallet).on_http(rpc_url);

    let id = provider.get_chain_id().await?;
    let chain_id = alloy_chains::Chain::from_id(id);

    let balance_owner_before = provider.get_balance(owner_wallet_address).await?;
    let balance_to_before = provider.get_balance(to_wallet_address).await?;

    let tx: TransactionRequest = TransactionRequest::default()
        .with_to(to_wallet_address)
        .with_value(U256::from(cli_options.amount_wei));

    println!("Running transactions on {chain_id}");
    let tx_hash = provider.send_transaction(tx).await?.watch().await?;
    println!("Sent transaction: {tx_hash}");

    let balance_owner_after = provider.get_balance(owner_wallet_address).await?;
    let balance_to_after = provider.get_balance(to_wallet_address).await?;

    let mut display_table = table!(
        [
            "Address",
            "Balance Before (ETH)",
            "Balance After (ETH)",
            "Difference (wei)"
        ],
        [
            truncate_middle(owner_wallet_address.to_string().as_str(), 10),
            alloy::primitives::utils::format_ether(balance_owner_before),
            alloy::primitives::utils::format_ether(balance_owner_after),
            balance_owner_before - balance_owner_after
        ],
        [
            truncate_middle(to_wallet_address.to_string().as_str(), 10),
            alloy::primitives::utils::format_ether(balance_to_before),
            alloy::primitives::utils::format_ether(balance_to_after),
            balance_to_after - balance_to_before
        ]
    );
    display_table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    display_table.printstd();

    Ok(())
}

fn truncate_middle(input: &str, max_len: usize) -> String {
    let len = input.len();
    if len <= max_len {
        return input.to_string();
    }

    let half_len = max_len / 2;
    let start = &input[..half_len];
    let end = &input[len - (max_len - half_len)..];

    format!("{}...{}", start, end)
}
