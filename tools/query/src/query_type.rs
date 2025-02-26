use crate::{
    account_queries::{get_account_balance_libra, get_tower_state, get_val_config},
    chain_queries::get_epoch,
    query_view::get_view,
};
use anyhow::{bail, Context, Result};
use diem_debugger::DiemDebugger;
use diem_sdk::{rest_client::Client, types::account_address::AccountAddress};
use indoc::indoc;
use libra_types::exports::AuthenticationKey;
use libra_types::type_extensions::client_ext::ClientExt;
use serde_json::json;

#[derive(Debug, clap::Subcommand)]
pub enum QueryType {
    /// Account balance
    Balance {
        #[clap(short, long)]
        /// account to query txs of
        account: AccountAddress,
    },
    /// User's Tower state
    Tower {
        #[clap(short, long)]
        /// account to query txs of
        account: AccountAddress,
    },
    /// A validator's on-chain configuration
    ValConfig {
        #[clap(short, long)]
        /// account to query txs of
        account: AccountAddress,
    },
    /// Epoch and waypoint
    Epoch,
    /// All account resources
    Resource {
        #[clap(short, long)]
        /// account to query txs of
        account: AccountAddress,
        #[clap(short, long)]
        /// the path of the resource, such as 0x1::slow_wallet::SlowWallet
        resource_path_string: String,
    },
    /// Execute a View function on-chain
    View {
        #[clap(
            short,
            long,
            help = indoc!{r#"
                Function identifier has the form <ADDRESS>::<MODULE_ID>::<FUNCTION_NAME>

                Example:
                0x1::ol_account::balance
            "#}
        )]
        function_id: String,

        #[clap(
            short,
            long,
            help = indoc!{r#"
                Type arguments separated by commas

                Example:
                'u8, u16, u32, u64, u128, u256, bool, address, vector<u8>, signer'
            "#}
        )]
        type_args: Option<String>,

        #[clap(
            short,
            long,
            help = indoc!{r#"
                Function arguments separated by commas

                Example:
                '0x1, true, 12, 24_u8, x"123456"'
            "#}
        )]
        args: Option<String>,
    },
    /// Looks up the address of an account given an auth key. The authkey diverges from the address after a key rotation.
    LookupAddress {
        #[clap(short, long)]
        auth_key: AuthenticationKey, // we use account address to parse, because that's the format needed to lookup users. AuthKeys and AccountAddress are the same formats.
    },
    /// How far behind the local is from the upstream nodes
    SyncDelay,

    Annotate {
        account: AccountAddress,
    }, // TODO:
       // /// Network block height
       // BlockHeight,
       // /// Get transaction history
       // Txs {
       //     #[clap(short, long)]
       //     /// account to query txs of
       //     account: AccountAddress,
       //     #[clap(long)]
       //     /// get transactions after this height
       //     txs_height: Option<u64>,
       //     #[clap(long)]
       //     /// limit how many txs
       //     txs_count: Option<u64>,
       //     #[clap(long)]
       //     /// filter by type
       //     txs_type: Option<String>,
       // },
       // /// Get events
       // Events {
       //     /// account to query events
       //     account: AccountAddress,
       //     /// switch for sent or received events.
       //     sent_or_received: bool,
       //     /// what event sequence number to start querying from, if DB does not have all.
       //     seq_start: Option<u64>,
       // },
}

impl QueryType {
    pub async fn query_to_json(&self, client_opt: Option<Client>) -> Result<serde_json::Value> {
        let client = match client_opt {
            Some(c) => c,
            None => Client::default().await?,
        };

        match self {
            QueryType::Balance { account } => {
                let res = get_account_balance_libra(&client, *account).await?;
                Ok(json!(res.scaled()))
            }
            QueryType::Tower { account } => {
                let res = get_tower_state(&client, *account).await?;
                Ok(json!(res))
            }
            QueryType::View {
                function_id,
                type_args,
                args,
            } => {
                let res =
                    get_view(&client, function_id, type_args.to_owned(), args.to_owned()).await?;
                let json = json!({ "body": res });
                Ok(json)
            }
            QueryType::Epoch => {
                let num = get_epoch(&client).await?;
                let json = json!({
                  "epoch": num,
                });
                Ok(json)
            }
            QueryType::LookupAddress { auth_key } => {
                let addr = client
                    .lookup_originating_address(auth_key.to_owned())
                    .await?;

                Ok(json!({ "address": addr }))
            }
            QueryType::Resource {
                account,
                resource_path_string,
            } => {
                let res = client
                    .get_account_resource(*account, resource_path_string)
                    .await?;

                if let Some(r) = res.inner() {
                    Ok(r.data.clone())
                } else {
                    bail!("no resource {resource_path_string}, found at address {account}");
                }
            }
            QueryType::ValConfig { account } => {
                let res = get_val_config(&client, *account).await?;

                // make this readable, turn the network address into a string
                Ok(json!({
                  "consensus_public_key": res.consensus_public_key,
                  "validator_network_addresses": res.validator_network_addresses().context("can't BCS decode the validator network address")?,
                  "fullnode_network_addresses": res.fullnode_network_addresses().context("can't BCS decode the fullnode network address")?,
                  "validator_index": res.validator_index,
                }))
            }
            QueryType::Annotate { account } => {
                let dbgger = DiemDebugger::rest_client(client)?;
                let version = dbgger.get_latest_version().await?;
                let blob = dbgger
                    .annotate_account_state_at_version(account.to_owned(), version)
                    .await?;
                if blob.is_none() {
                    bail!("cannot find account state at {}", account)
                };
                // dbg!(&blob.unwrap());
                // blob.unwrap().to_string();
                let pretty = format!("{:#}", blob.unwrap().to_string());
                Ok(json!(pretty))
            }
            _ => {
                bail!(
                    "Not implemented for type: {:?}\n Ground control to Major Tom.",
                    self
                )
            }
        }
    }
}
