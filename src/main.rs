use std::time::Duration;

use reqwest::Client;
use serde_json::json;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let client = Client::new();
    let platform_url = "https://ethereum-holesky-rpc.publicnode.com";
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

    let rollup_id = "rollup_id_2";
    let rpc_urls = [
        "http://34.64.94.33:5000",
        "http://34.47.93.98:5000",
        "http://34.64.32.56:5000",
        "http://34.64.46.56:5000",
        "http://34.47.120.77:5000",
    ];
    let tx_orderer_addresses = [
        "0x13a8800770f81731F45E7b33D6761FD6f08A70f7",
        "0x5D51044C4cB62280EF1700F2E7378e1198648a52",
        "0xc6bA578acFF1eA914A6a727b2F20776eB4ad61EE",
        "0xFf86a44c0c3e73636a8Da7eA272E80f1B87E843a",
        "0x50D1ed3FfaD13a1af7D0E1Cfa02461985b4e500f",
    ];

    let l1_block_generation_interval = 12;
    let block_generation_interval = 3;

    let mut rollup_block_height = 1;
    let mut block_generation_count = 0;

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

    let response = response.json::<serde_json::Value>().await.unwrap();

    if let Some(hex_str) = response["result"].as_str() {
        match u64::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
            Ok(mut platform_block_height) => loop {
                let leader_index = (rollup_block_height) % tx_orderer_addresses.len();
                let next_leader_index = (leader_index + 1) % tx_orderer_addresses.len();

                println!("Leader index: {}", leader_index);

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

                println!(
                    "Request platform_block_height: {} / rollup_block_height: {}",
                    platform_block_height, rollup_block_height
                );

                match client
                    .post(rpc_urls[leader_index])
                    .json(&request_body)
                    .send()
                    .await
                {
                    Ok(response) => {
                        let request_body = json!({
                            "jsonrpc": "2.0",
                            "method": "get_raw_transaction_list",
                            "params": {
                                "rollup_id": rollup_id,
                                "rollup_block_height": rollup_block_height,
                            },
                            "id": 1
                        });

                        rollup_block_height += 1;

                        loop {
                            match client
                                .post(rpc_urls[rollup_block_height % rpc_urls.len()])
                                .json(&request_body)
                                .send()
                                .await
                            {
                                Ok(response) => {
                                    let response =
                                        response.json::<serde_json::Value>().await.unwrap();

                                    if response["error"].is_null() {
                                        break;
                                    } else {
                                        eprintln!("Request failed: {:?}", response);
                                    }
                                }
                                Err(e) => eprintln!("Request failed: {}", e),
                            }
                        }
                    }
                    Err(e) => eprintln!("Request failed: {}", e),
                }

                if block_generation_count
                    == l1_block_generation_interval / block_generation_interval
                {
                    block_generation_count = 0;
                    platform_block_height += 1;
                }

                block_generation_count += 1;
                sleep(Duration::from_secs(block_generation_interval)).await;
            },
            Err(e) => println!("Failed to convert hex to u64: {}", e),
        }
    }
    return;
}
