use std::env;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde_json::json;
use tokio::time::sleep;

use tracing::{error, info};

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_millis()
}

fn dump_rollup_experiment_meta(experiment_end_epoch_ms: u128, reason: &'static str) {
    let path = "rollup_experiment_meta.json";
    let value = serde_json::json!({
        "experiment_end_epoch_ms": experiment_end_epoch_ms,
        "experiment_end_reason": reason,
    });
    match File::create(path) {
        Ok(f) => {
            if let Err(e) = serde_json::to_writer_pretty(f, &value) {
                eprintln!("failed to write {}: {}", path, e);
            }
        }
        Err(e) => eprintln!("failed to create {}: {}", path, e),
    }
    println!(
        "ROLLUP_EXPERIMENT_META experiment_end_epoch_ms={} experiment_end_reason={}",
        experiment_end_epoch_ms, reason
    );
    tracing::info!(
        target: "rollup_experiment",
        "rollup experiment meta: end={} reason={}",
        experiment_end_epoch_ms, reason
    );
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = env::args().collect();
    let max_tx_count: usize = args
        .get(1)
        .expect("Usage: test-client-rs <max_tx_count> <time_limit_secs>")
        .parse()
        .expect("max_tx_count must be a positive integer");
    let time_limit_secs: u64 = args
        .get(2)
        .expect("Usage: test-client-rs <max_tx_count> <time_limit_secs>")
        .parse()
        .expect("time_limit_secs must be a non-negative integer");
    println!(
        "max_tx_count: {}, time_limit_secs: {}",
        max_tx_count, time_limit_secs
    );

    let client = Client::new();
    let platform_url = "http://127.0.0.1:8545";
    let executor_address = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";

    /*
       let rollup_id = "radius_rollup_sign";
       let rpc_urls = [
           "http://34.47.120.77:5000",
           "http://34.64.46.56:5000",
           "http://34.64.32.56:5000",
           "http://34.64.94.33:5000",
           "http://34.47.93.98:5000",
       ];
       let tx_orderer_addresses = [
           "0x4b3dd373002d4626cf5f535c0a170f724217490c",
           "0xd18d378823a7d2c227dfbe92f6e8fb2f1fe7b3d6",
           "0xb4cffa6aa0062e93f387be8c441297454de7c675",
           "0xc325eee9b01bce01c59b38440a2e1be3196327e2",
           "0x6f3b1562efba69885a6156bb5ecde30bcf5a8a1e",
       ];
    */

    let rollup_id = "radius_rollup";
    let rpc_urls = [
        "http://165.194.35.15:11103", // sys5(TX_ORDERER)
        "http://165.194.35.11:11103", // sys2(TX_ORDERER_2)
        "http://165.194.35.11:11106", // sys2(TX_ORDERER_3)
        "http://165.194.35.14:11103", // sys4(TX_ORDERER_4)
        "http://165.194.35.14:11106", // sys4(TX_ORDERER_5)
    ];
    let tx_orderer_addresses = [
        "0xa0Ee7A142d267C1f36714E4a8F75612F20a79720", // sys5(TX_ORDERER)
        "0xcd3B766CCDd6AE721141F452C550Ca635964ce71", // sys2(TX_ORDERER_2)
        "0x2546BcD3c84621e976D8185a91A922aE77ECEc30", // sys2(TX_ORDERER_3)
        "0xbDA5747bFD65F08deb54cb465eB87D40e51B197E", // sys4(TX_ORDERER_4)
        "0xdD2FD4581271e230360230F9337D5c0430Bf44C0", // sys4(TX_ORDERER_5)
    ];

    let l1_block_generation_interval = 12;
    let block_generation_interval = 3;

    let mut rollup_block_height = 1;
    let mut block_generation_count = 0;
    
    let mut cumulative_tx_count = 0;
    let mut tx_arrival_log: Vec<(serde_json::Value, u128)> = Vec::new();

    let get_platform_block_height = json!({
        "jsonrpc":"2.0",
        "method":"eth_blockNumber",
        "params": [],
        "id":1
    });

    let response = client
        .post(platform_url)
        .json(&get_platform_block_height)
        .send()
        .await
        .unwrap();

    let epoch = 0; // new code

    let response = response.json::<serde_json::Value>().await.unwrap();

    if let Some(hex_str) = response["result"].as_str() {
        match u64::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
            Ok(mut platform_block_height) => {
                let experiment_start = Instant::now();
                let time_limit = Duration::from_secs(time_limit_secs);
                let (experiment_end_epoch_ms, experiment_end_reason) = loop {
                    if experiment_start.elapsed() >= time_limit {
                        println!(
                            "Time limit ({}s) reached. Stopping experiment. arrival_log len: {}",
                            time_limit_secs,
                            tx_arrival_log.len()
                        );
                        break (epoch_ms(), "time_limit");
                    }

                let current_leader_tx_orderer_index =
                    (rollup_block_height) % tx_orderer_addresses.len();
                let next_leader_tx_orderer_index =
                    (current_leader_tx_orderer_index + 1) % tx_orderer_addresses.len();

                info!(
                    "Current leader tx orderer / next leader tx orderer\ncurrent_leader={}\nnext_leader={}\nepoch={} -> {}",
                    tx_orderer_addresses[current_leader_tx_orderer_index],
                    tx_orderer_addresses[next_leader_tx_orderer_index],
                    epoch,
                    epoch + 1,
                );

                /*
                let request_body = json!({
                            "jsonrpc": "2.0",
                            "method": "finalize_block",
                            "params": {
                                "finalize_block_message": {
                                    "rollup_id": rollup_id,
                                    "executor_address": executor_address,
                                    "platform_block_height": platform_block_height,
                                    "rollup_block_height": rollup_block_height,
                                    "block_creator_address":
                tx_orderer_addresses[leader_index],
                "next_block_creator_address": tx_orderer_addresses[next_leader_index],
                                },
                                "signature": ""
                            },
                            "id": 1
                        });
                */

                let request_body = json!({
                    "jsonrpc": "2.0",
                    "method": "get_raw_transaction_list",
                    "params": {
                        "leader_change_message": {
                            "current_leader_tx_orderer_address": tx_orderer_addresses[current_leader_tx_orderer_index],
                            "executor_address": executor_address,
                            "next_leader_tx_orderer_address": tx_orderer_addresses[next_leader_tx_orderer_index],
                            "platform_block_height": platform_block_height - 3,
                            "rollup_id": rollup_id,
                        },
                        "rollup_signature": "0xc6bA578acFF1eA914A6a727b2F20776eB4ad61EE333333333333333333333333c6bA578acFF1eA914A6a727b2F20776eB4ad61EE33333333333333333333333333"
                    },
                    "id": 1
                });

                println!(
                    "Request platform_block_height: {} / rollup_block_height: {}",
                    platform_block_height, rollup_block_height
                );

                match client
                    .post(rpc_urls[current_leader_tx_orderer_index])
                    .json(&request_body)
                    .send()
                    .await
                {
                    Ok(response) => {
                        let arrived_at_epoch_ms = epoch_ms();
                        let response = response.json::<serde_json::Value>().await.unwrap();

                        let tx_list_len = response["result"]["raw_transaction_list"]
                            .as_array()
                            .map(|arr| {
                                for tx in arr {
                                    tx_arrival_log.push((tx.clone(), arrived_at_epoch_ms));
                                }
                                arr.len()
                            })
                            .unwrap_or(0);
                        cumulative_tx_count += tx_list_len;
                        println!(
                            "raw_transaction_list 길이: {}, 누적 합: {}, arrival_log 길이: {}",
                            tx_list_len, cumulative_tx_count, tx_arrival_log.len()
                        );

                        if let Ok(pretty) = serde_json::to_string_pretty(&response) {
                            println!("Response\n{}", pretty);
                        } else {
                            println!("Response {:?}", response);
                        }

                        rollup_block_height += 1;

                        if tx_arrival_log.len() >= max_tx_count {
                            println!(
                                "Reached max_tx_count ({}). Stopping experiment.",
                                max_tx_count
                            );
                            break (epoch_ms(), "max_tx_count");
                        }
                    }
                    Err(e) => error!("Request failed: {}", e),
                }

                if block_generation_count
                    == l1_block_generation_interval / block_generation_interval
                {
                    block_generation_count = 0;
                    platform_block_height += 1;
                }

                block_generation_count += 1;
                sleep(Duration::from_secs(block_generation_interval)).await;
                };

                dump_rollup_experiment_meta(experiment_end_epoch_ms, experiment_end_reason);
            }
            Err(e) => println!("Failed to convert hex to u64: {}", e),
        }
    }

    let log_filename = format!("tx_arrival_log_{}.csv", epoch_ms());
    let mut file = File::create(&log_filename).expect("Failed to create log file");
    writeln!(file, "index,arrived_at_epoch_ms,raw_transaction").unwrap();
    for (i, (tx, arrived_at_epoch_ms)) in tx_arrival_log.iter().enumerate() {
        writeln!(file, "{},{},{}", i, arrived_at_epoch_ms, tx).unwrap();
    }
    println!(
        "Saved {} entries to {}",
        tx_arrival_log.len(),
        log_filename
    );
}