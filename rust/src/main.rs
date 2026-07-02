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
const RPC_USER: &str = "Alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.

#[derive(Deserialize)]
struct ScriptPubKey {
    address: Option<String>,
    addresses: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VinPrevout {
    value: Option<f64>,
    #[serde(rename = "scriptPubKey")]
    script_pub_key: Option<ScriptPubKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")] 
struct Vin {
    prevout: Option<VinPrevout>,
}

#[derive(Deserialize)]
struct Vout {
    value: f64,
    #[serde(rename = "scriptPubKey")]
    script_pub_key: ScriptPubKey,
}
#[derive(Deserialize)]
struct DecodedTx {
    vin: Vec<Vin>,
    vout: Vec<Vout>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerboseTransaction {
    fee: Option<f64>,
    blockheight: Option<u32>,
    blockhash: Option<String>,
    decoded: DecodedTx,
}

fn address_from_spk(spk: &ScriptPubKey) -> String {
    spk.address
        .clone()
        .or_else(|| spk.addresses.as_ref().and_then(|a: &Vec<String>| a.first().cloned()))
        .expect("address missing from scriptPubKey")
}

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

    let miner_rpc = Client::new(
        &format!("{}/wallet/{}", RPC_URL, "Miner"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let trader_rpc = Client::new(
        &format!("{}/wallet/{}", RPC_URL, "Trader"), 
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
    let miner_address = miner_rpc.get_new_address(Some("Mining Reward"), None)?.assume_checked();
    rpc.generate_to_address(101, &miner_address)?;  // 101 blocks is generated to because of the coinbase maturity consesus rule of the coinbase transaction that cannot be spent until the minimum output of 100 is confirmed
    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Miner Balance: {}", miner_balance); 

    // Load Trader wallet and generate a new address
    let trader_address = trader_rpc.get_new_address(Some("Received"), None)?.assume_checked();
    println!("Trader Address: {}", trader_address);

    // Send 20 BTC from Miner to Trader
    let btc: f64 = 20.0;
    let amount_to_send = Amount::from_btc(btc).unwrap();
    let mut txid;
    if miner_balance > amount_to_send {
        let result = rpc.send_to_address(
            &trader_address,
            amount_to_send,
            None,
            None,
            None,
            None,
            None,
            None,
        )?;
        println!("Transaction sent with ID: {}", result);
        txid = result
    } else {
        panic!("Miner wallet has insufficient balance to send 20 BTC");
    }

    // Check transaction in mempool
    let tx = &txid;
    let mempool_info = rpc.get_mempool_entry(&tx)?;
    println!("Mempool Info: {:?}", mempool_info);

    // Mine 1 block to confirm the transaction
    rpc.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details
    let tx_details = miner_rpc.call::<VerboseTransaction>(
        "gettransaction",
        &[json!(txid), json!(null), json!(true)],
    )?;
    let vin = &tx_details.decoded.vin[0];
    let prevout = vin.prevout.as_ref().expect("vin prevout missing");
    let miner_input_address = address_from_spk(
        prevout.script_pub_key.as_ref().expect("prevout scriptPubKey missing"),
    );
    let miner_input_amount = prevout.value.expect("prevout value missing");
    let trader_vout = &tx_details.decoded.vout[0];
    let trader_input_address = address_from_spk(&trader_vout.script_pub_key);
    let trader_input_amount = trader_vout.value;
    let change_vout = &tx_details.decoded.vout[1];
    let miner_change_address = address_from_spk(&change_vout.script_pub_key);
    let miner_change_amount = change_vout.value;
    let fee = tx_details.fee.expect("fee missing").abs();
    let block_height = tx_details.blockheight.expect("blockheight missing");
    let block_hash = tx_details.blockhash.expect("blockhash missing");


    // Write the data to ../out.txt in the specified format given in readme.md
    let mut file = File::create("../out.txt")?;
    file.write_all(format!("txid: {}\n", txid).as_bytes())?;
    file.write_all(format!("miner_input_address: {}\n", miner_input_address).as_bytes())?;
    file.write_all(format!("miner_input_amount: {}\n", miner_input_amount).as_bytes())?;
    file.write_all(format!("trader_input_address: {}\n", trader_input_address).as_bytes())?;
    file.write_all(format!("trader_input_amount: {}\n", trader_input_amount).as_bytes())?;
    file.write_all(format!("miner_change_address: {}\n", miner_change_address).as_bytes())?;
    file.write_all(format!("miner_change_amount: {}\n", miner_change_amount).as_bytes())?;
    file.write_all(format!("fee: {}\n", fee).as_bytes())?;
    file.write_all(format!("block_height: {}\n", block_height).as_bytes())?;
    file.write_all(format!("block_hash: {}\n", block_hash).as_bytes())?;
    println!("Data written to ../out.txt");

    Ok(())
}
