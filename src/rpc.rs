use anyhow::Error;
use cln_plugin::Plugin;
use cln_rpc::RpcError;
use serde_json::{json, Map};

use crate::{fetch::resolve_invstring, hooks::Paycmd};

const PAYANYARGS: [&str; 3] = ["invstring", "amount_msat", "message"];

pub async fn payany(
    plugin: Plugin<()>,
    args: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let mut params = Map::new();
    if let Some(args_obj) = args.as_object() {
        params = args_obj.clone();
    } else if let Some(args_arr) = args.as_array() {
        for (i, arg) in args_arr.iter().enumerate() {
            params.insert(PAYANYARGS[i].to_string(), arg.clone());
        }
    }
    let actual_params = match resolve_invstring(plugin, &mut params, Paycmd::Xpay).await {
        Ok(o) => o,
        Err(e) => {
            log::info!("Error fetching invoice: {}", e);
            params.remove("message");
            return Ok(json!(RpcError {
                code: None,
                message: e.to_string(),
                data: None,
            }));
        }
    };
    Ok(json!({"invoice":format!("{}", actual_params.get("invstring").unwrap().as_str().unwrap())}))
}
