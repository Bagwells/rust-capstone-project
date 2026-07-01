#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "bagwellsrpc";
const RPC_PASS: &str = "bitcoinrpc";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    let wallet_names = vec!["Miner", "Trader"];
    let loaded_wallets = rpc.list_wallets()?;

    for wallet in wallet_names {
        if loaded_wallets.contains(&wallet.to_string()) {
            rpc.load_wallet(&wallet.to_string())?;
            println!("{wallet} is loaded");
        } else {
            if rpc.load_wallet(wallet).is_err() {
                rpc.create_wallet(
                    &wallet.to_string(),
                    None,
                    None,
                    None,
                    None
            )?;
            }
            println!("{wallet} is created and loaded");
        }
    }

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_address = rpc.get_new_address(Some("Mining Reward"), None)?;
    rpc.generate_to_address(101, &miner_address)?;  // 101 blocks is generated to because of the coinbase maturity consesus rule of the coinbase transaction that cannot be spent until the minimum output of 100 is confirmed
    let miner_balance = rpc.get_balance(Some(&wallet_names[0]), None)?;
    println!("Miner Balance: {}", miner_balance); 

    // Load Trader wallet and generate a new address
    let trader_address = rpc.get_new_address(Some("Received"), None)?;
    println!("Trader Address: {}", trader_address);

    // Send 20 BTC from Miner to Trader
    let btc: f64 = 20.0;
    let amount_to_send = Amount::from_btc(btc).unwrap();
    let mut txid = String::new();
    if miner_balance > amount_to_send {
        txid = send_to_address(&trader_address, &btc)?;
        println!("Transaction sent");
    } else {
        println!("Miner wallet has insufficient balance to send 20 BTC");
    }

    // Check transaction in mempool
    let mempool_info = rpc.get_mempool_entry(&txid)?;
    println!("Mempool Info: {:?}", mempool_info);

    // Mine 1 block to confirm the transaction
    rpc.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details
    let tx_details = rpc.get_transaction(&txid, None)?;
    println!("{}", tx_details);

    // Write the data to ../out.txt in the specified format given in readme.md
    

    Ok(())
}
