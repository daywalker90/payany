use std::{collections::HashMap, str::FromStr, sync::Arc, time::Instant};

use anyhow::anyhow;
use cln_rpc::primitives::Amount;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

pub const URI_SCHEMES: [&str; 3] = ["lightning:", "lno:", "lnurl:"];

#[derive(Debug, Clone)]
pub struct PluginState {
    pub config: Arc<Mutex<Config>>,
    pub pay_index: Arc<Mutex<u64>>,
    pub budget_lock: Arc<tokio::sync::Mutex<()>>,
    pub budget_reserved: Arc<Mutex<HashMap<String, (u64, Instant)>>>,
}
impl Default for PluginState {
    fn default() -> PluginState {
        PluginState {
            config: Arc::new(Mutex::new(Config::default())),
            pay_index: Arc::new(Mutex::new(0)),
            budget_lock: Arc::new(tokio::sync::Mutex::new(())),
            budget_reserved: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub budget_per: Option<u64>,
    pub budget_amount_msat: Option<Amount>,
    pub xpay_handle_pay: bool,
    pub payargs: Vec<String>,
    pub xpayargs: Vec<String>,
    pub renepayargs: Vec<String>,
    pub strict_lnurl: bool,
    pub tor_proxy: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Paycmd {
    Pay,
    Xpay,
    Renepay,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RpcCommand {
    pub rpc_command: RpcDetails,
}
#[derive(Deserialize, Serialize, Debug)]
pub struct RpcDetails {
    pub id: serde_json::Value,
    pub method: String,
    pub params: ParamValue,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum ParamValue {
    Array(Vec<serde_json::Value>),
    Object(Map<String, serde_json::Value>),
    String(String),
}
impl ParamValue {
    pub fn to_object(
        &self,
        paycmd: Paycmd,
        config: &Config,
    ) -> Result<Map<String, serde_json::Value>, anyhow::Error> {
        let mut params: Map<String, serde_json::Value> = Map::new();
        match self {
            ParamValue::Array(p_arr) => match paycmd {
                Paycmd::Pay => {
                    if p_arr.len() > config.payargs.len() {
                        return Err(anyhow!("payany: too many arguments for pay"));
                    }
                    for (i, val) in p_arr.iter().enumerate() {
                        params.insert(config.payargs[i].clone(), val.clone());
                    }
                }
                Paycmd::Xpay => {
                    if p_arr.len() > config.xpayargs.len() {
                        return Err(anyhow!("payany: too many arguments for xpay"));
                    }
                    for (i, val) in p_arr.iter().enumerate() {
                        params.insert(config.xpayargs[i].clone(), val.clone());
                    }
                }
                Paycmd::Renepay => {
                    if p_arr.len() > config.renepayargs.len() {
                        return Err(anyhow!("payany: too many arguments for renepay"));
                    }
                    for (i, val) in p_arr.iter().enumerate() {
                        params.insert(config.renepayargs[i].clone(), val.clone());
                    }
                }
            },
            ParamValue::Object(map) => map.clone_into(&mut params),
            ParamValue::String(str) => match paycmd {
                Paycmd::Pay => {
                    params.insert(config.payargs[0].clone(), json!(str));
                }
                Paycmd::Xpay => {
                    params.insert(config.xpayargs[0].clone(), json!(str));
                }
                Paycmd::Renepay => {
                    params.insert(config.renepayargs[0].clone(), json!(str));
                }
            },
        }

        for param in params.keys() {
            match paycmd {
                Paycmd::Pay => {
                    if !config.payargs.contains(param) && param != "bolt11" {
                        return Err(anyhow!("unknown `pay` param: {param}"));
                    }
                }
                Paycmd::Xpay => {
                    if !config.xpayargs.contains(param) {
                        return Err(anyhow!("unknown `xpay` param: {param}"));
                    }
                }
                Paycmd::Renepay => {
                    if !config.renepayargs.contains(param) {
                        return Err(anyhow!("unknown `renepay` param: {param}"));
                    }
                }
            }
        }

        match paycmd {
            Paycmd::Pay => {
                if !params.contains_key("invstring") && !params.contains_key("bolt11") {
                    return Err(anyhow!(
                        "missing required parameter for `pay`: `invstring`/`bolt11`"
                    ));
                }
            }
            Paycmd::Xpay => {
                if !params.contains_key(&config.xpayargs[0]) {
                    return Err(anyhow!(
                        "missing required parameter for `xpay`: {}",
                        config.xpayargs[0]
                    ));
                }
            }
            Paycmd::Renepay => {
                if !params.contains_key(&config.renepayargs[0]) {
                    return Err(anyhow!(
                        "missing required parameter for `renepay`: {}",
                        config.renepayargs[0]
                    ));
                }
            }
        }
        Ok(params)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LnurlpConfig {
    pub callback: String,
    #[serde(rename = "maxSendable")]
    pub max_sendable: u64,
    #[serde(rename = "minSendable")]
    pub min_sendable: u64,
    pub metadata: String,
    pub tag: String,
    #[serde(rename = "commentAllowed")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_allowed: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LnurlpCallback {
    pub pr: String,
    pub routes: Vec<String>,
}

#[derive(Debug)]
pub enum TimeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
}

impl FromStr for TimeUnit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "second" | "seconds" | "sec" | "secs" | "s" => Ok(TimeUnit::Second),
            "minute" | "minutes" | "min" | "mins" | "m" => Ok(TimeUnit::Minute),
            "hour" | "hours" | "h" => Ok(TimeUnit::Hour),
            "day" | "days" | "d" => Ok(TimeUnit::Day),
            "week" | "weeks" | "w" => Ok(TimeUnit::Week),
            _ => Err(format!("Unsupported time unit: {s}")),
        }
    }
}
