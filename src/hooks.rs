use anyhow::Error;
use cln_plugin::Plugin;
use cln_rpc::RpcError;
use serde_json::{json, Map};

use crate::fetch::resolve_invstring;

const XPAYARGS: [&str; 7] = [
    "invstring",
    "amount_msat",
    "maxfee",
    "layers",
    "retry_for",
    "partial_msat",
    "message",
];

const PAYARGS: [&str; 14] = [
    "bolt11",
    "amount_msat",
    "label",
    "riskfactor",
    "maxfeepercent",
    "retry_for",
    "maxdelay",
    "exemptfee",
    "localinvreqid",
    "exclude",
    "maxfee",
    "description",
    "partial_msat",
    "message",
];

#[derive(Clone, Copy)]
pub enum Paycmd {
    Pay,
    Xpay,
}

pub async fn hook_handler(
    plugin: Plugin<()>,
    args: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let rpc_command = if let Some(rpc_val) = args.get("rpc_command") {
        rpc_val.as_object().unwrap()
    } else {
        return Ok(json!({"result":"continue"}));
    };
    let method = if let Some(method_vak) = rpc_command.get("method") {
        method_vak.as_str().unwrap()
    } else {
        return Ok(json!({"result":"continue"}));
    };
    let paycmd = match method {
        "xpay" => Paycmd::Xpay,
        "pay" => Paycmd::Pay,
        _ => return Ok(json!({"result":"continue"})),
    };
    let params_val = if let Some(pv) = rpc_command.get("params") {
        pv
    } else {
        return Ok(json!({"result":"continue"}));
    };

    log::debug!("params: {}", params_val);
    let mut params: Map<String, serde_json::Value> = Map::new();
    if let Some(p) = params_val.as_object() {
        params = p.clone();
    } else if let Some(p_arr) = params_val.as_array() {
        match paycmd {
            Paycmd::Pay => {
                if p_arr.len() > PAYARGS.len() {
                    log::info!("too many arguments: {}>{}", p_arr.len(), XPAYARGS.len());
                    return Ok(json!({"return":{"error":json!(RpcError {
                        code: Some(-32602),
                        message: "payany: too many arguments".to_string(),
                        data: None,
                    })}}));
                }
                for (i, val) in p_arr.iter().enumerate() {
                    params.insert(PAYARGS[i].to_string(), val.clone());
                }
            }
            Paycmd::Xpay => {
                if p_arr.len() > XPAYARGS.len() {
                    log::info!("too many arguments: {}>{}", p_arr.len(), XPAYARGS.len());
                    return Ok(json!({"return":{"error":json!(RpcError {
                        code: Some(-32602),
                        message: "payany: too many arguments".to_string(),
                        data: None,
                    })}}));
                }
                for (i, val) in p_arr.iter().enumerate() {
                    params.insert(XPAYARGS[i].to_string(), val.clone());
                }
            }
        }
    } else {
        log::debug!("no params obj! {}", params_val);
        return Ok(json!({"result":"continue"}));
    };
    if params.is_empty() {
        log::debug!("empty params! {}", params_val);
        return Ok(json!({"result":"continue"}));
    }

    let mut actual_params = match resolve_invstring(plugin, &mut params, paycmd).await {
        Ok(o) => o,
        Err(e) => {
            log::info!("Error fetching invoice: {}", e);
            params.remove("message");
            return Ok(json!({"return": {"error":json!(RpcError {
                code: Some(-32602),
                message: format!("payany could not fetch invoice: {}", e),
                data: None,
            })}}));
        }
    };
    actual_params.remove("message");
    Ok(json!({"replace": {"jsonrpc":"2.0",
            "id": rpc_command.get("id").unwrap(),
            "method":format!("{}",match paycmd{
                Paycmd::Pay => "pay",
                Paycmd::Xpay => "xpay",
            }),
            "params":actual_params}}))
}
