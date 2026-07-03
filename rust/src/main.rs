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
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.

#[derive(Deserialize)]
#[serde(untagged)]
enum AddressField {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
struct ScriptPubKey {
    address: Option<AddressField>,
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
    txid: Option<String>,
    vout: Option<u32>,
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

fn address_from_scriptpubkey(spk: &ScriptPubKey) -> String {
    // try "address" field first (string or array)
    if let Some(field) = &spk.address {
        return match field {
            AddressField::One(s) => s.clone(),
            AddressField::Many(v) => v
                .first()
                .expect("address array was empty")
                .clone(),
        };
    }
    // fallback to legacy "addresses" array
    spk.addresses
        .as_ref()
        .and_then(|a| a.first().cloned())
        .expect("address missing from scriptPubKey")
}

fn scriptpubkey_from_value(v: &serde_json::Value) -> ScriptPubKey {
    serde_json::from_value(v["scriptPubKey"].clone()).expect("scriptPubKey parse failed")
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
    let blockchain_info: serde_json::Value = rpc.call("getblockchaininfo", &[])?;
    println!("Blockchain Info: {}", blockchain_info.to_string());

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    let wallet_names = vec!["Miner", "Trader"];
    let loaded_wallets = rpc.list_wallets()?;

    for wallet in wallet_names {
        if loaded_wallets.contains(&wallet.to_string()) {
            println!("{wallet} is already loaded");
        } else {
            if rpc.load_wallet(wallet).is_err() {
                rpc.create_wallet(&wallet.to_string(), None, None, None, None)?;
            }
            println!("{wallet} is loaded");
        }
    }

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_address = miner_rpc
        .get_new_address(Some("Mining Reward"), None)?
        .assume_checked();
    rpc.generate_to_address(10, &miner_address)?; // 101 blocks is generated to because of the coinbase maturity consesus rule of the coinbase transaction that cannot be spent until the minimum output of 100 is confirmed
    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Miner Balance: {}", miner_balance);

    // Load Trader wallet and generate a new address
    let trader_address = trader_rpc
        .get_new_address(Some("Received"), None)?
        .assume_checked();
    println!("Trader Address: {}", trader_address);

    // Send 20 BTC from Miner to Trader
    let btc: f64 = 20.0;
    let amount_to_send = Amount::from_btc(btc).unwrap();
    let mut txid;
    if miner_balance > amount_to_send {
        let result = miner_rpc.send_to_address(
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
    rpc.generate_to_address(101, &miner_address)?;

    // Extract all required transaction details
    let tx_details = miner_rpc.call::<VerboseTransaction>(
        "gettransaction",
        &[json!(txid.to_string()), json!(null), json!(true)],
    )?;
    let vin = &tx_details.decoded.vin[0];
    let prev_txid = vin.txid.as_ref().expect("vin txid missing");
    let prev_vout = vin.vout.expect("vin vout missing") as usize;
    let prev: serde_json::Value =
        miner_rpc.call("gettransaction", &[json!(prev_txid), json!(null), json!(true)])?;
    let prev_output = &prev["decoded"]["vout"][prev_vout];
    let miner_input_address = address_from_scriptpubkey(&scriptpubkey_from_value(prev_output));
    let miner_input_amount = prev_output["value"].as_f64().unwrap();
    let trader_str = trader_address.to_string();
    let trader_vout = tx_details
        .decoded
        .vout
        .iter()
        .find(|v| address_from_scriptpubkey(&v.script_pub_key) == trader_str)
        .expect("trader output not found");
    let change_vout = tx_details
        .decoded
        .vout
        .iter()
        .find(|v| address_from_scriptpubkey(&v.script_pub_key) != trader_str)
        .expect("change output not found");
    let trader_input_address = address_from_scriptpubkey(&trader_vout.script_pub_key);
    let trader_input_amount = trader_vout.value;
    let miner_change_address = address_from_scriptpubkey(&change_vout.script_pub_key);
    let miner_change_amount = change_vout.value;
    let fee = tx_details.fee.expect("fee missing").abs();
    let block_height = tx_details.blockheight.expect("blockheight missing");
    let block_hash = tx_details.blockhash.expect("blockhash missing");
    // Write the data to ../out.txt (10 raw lines, no labels)
    let mut file = File::create("../out.txt")?;
    writeln!(file, "{}", txid)?;
    writeln!(file, "{}", miner_input_address)?;
    writeln!(file, "{}", miner_input_amount)?;
    writeln!(file, "{}", trader_input_address)?;
    writeln!(file, "{}", trader_input_amount)?;
    writeln!(file, "{}", miner_change_address)?;
    writeln!(file, "{}", miner_change_amount)?;
    writeln!(file, "{}", fee)?;
    writeln!(file, "{}", block_height)?;
    writeln!(file, "{}", block_hash)?;
    println!("Data written to ../out.txt");

    Ok(())
}
