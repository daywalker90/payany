use std::path::Path;

use anyhow::anyhow;
use cln_plugin::{options, ConfiguredPlugin, Plugin};
use cln_rpc::{
    model::requests::{
        AskrenecreatelayerRequest, AskrenedisablenodeRequest, AskreneremovelayerRequest,
        AskreneupdatechannelRequest, DecodeRequest, HelpRequest,
    },
    primitives::{Amount, PublicKey, ShortChannelIdDir},
    ClnRpc, RpcError,
};
use serde_json::{json, Map};

use crate::{
    structs::{Config, TimeUnit},
    PluginState, OPT_PAYANY_BUDGET_AMOUNT_MSAT, OPT_PAYANY_BUDGET_PER, OPT_PAYANY_DNS,
    OPT_PAYANY_HANDLE_PAY, OPT_PAYANY_STRICT_LNURL,
};

fn parse_time_period(input: &str) -> Result<u64, anyhow::Error> {
    let re = regex::Regex::new(r"(\d+)\s*([a-zA-Z]+)")?;
    if let Some(caps) = re.captures(input) {
        let value: u64 = caps[1].parse()?;
        let unit = &caps[2].to_lowercase();

        if let Ok(time_unit) = unit.parse() {
            match time_unit {
                TimeUnit::Second => Ok(value),
                TimeUnit::Minute => Ok(value * 60),
                TimeUnit::Hour => Ok(value * 60 * 60),
                TimeUnit::Day => Ok(value * 60 * 60 * 24),
                TimeUnit::Week => Ok(value * 60 * 60 * 24 * 7),
            }
        } else {
            Err(anyhow!(format!("Unsupported time unit: {}", unit)))
        }
    } else {
        Err(anyhow!("Invalid time format: {}", input))
    }
}

pub fn get_startup_options(
    plugin: &ConfiguredPlugin<PluginState, tokio::io::Stdin, tokio::io::Stdout>,
    state: PluginState,
) -> Result<(), anyhow::Error> {
    {
        let mut config = state.config.lock();

        if let Some(bamt) = plugin.option_str(OPT_PAYANY_BUDGET_AMOUNT_MSAT)? {
            check_option(&mut config, OPT_PAYANY_BUDGET_AMOUNT_MSAT, &bamt)?;
        };
        if let Some(bper) = plugin.option_str(OPT_PAYANY_BUDGET_PER)? {
            check_option(&mut config, OPT_PAYANY_BUDGET_PER, &bper)?;
        };
        if let Some(handle) = plugin.option_str(OPT_PAYANY_HANDLE_PAY)? {
            check_option(&mut config, OPT_PAYANY_HANDLE_PAY, &handle)?;
        };
        if let Some(bper) = plugin.option_str(OPT_PAYANY_DNS)? {
            check_option(&mut config, OPT_PAYANY_DNS, &bper)?;
        };
        if let Some(handle) = plugin.option_str(OPT_PAYANY_STRICT_LNURL)? {
            check_option(&mut config, OPT_PAYANY_STRICT_LNURL, &handle)?;
        };
        if config.budget_amount_msat.is_some() || config.budget_per.is_some() {
            if config.budget_amount_msat.is_some() && config.budget_per.is_some() {
                log::info!(
                    "Budget set to {}msat every {}seconds",
                    config.budget_amount_msat.unwrap().msat(),
                    config.budget_per.unwrap()
                );
            } else {
                return Err(anyhow!("Incomplete Budget options!"));
            }
        } else {
            log::info!("No Budget set!")
        }
    }
    Ok(())
}

pub async fn setconfig_callback(
    plugin: Plugin<PluginState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, anyhow::Error> {
    let name = args
        .get("config")
        .ok_or_else(|| anyhow!("Bad CLN object. No option name found!"))?
        .as_str()
        .ok_or_else(|| anyhow!("Bad CLN object. Option name not a string!"))?;
    let value = args
        .get("val")
        .ok_or_else(|| anyhow!("Bad CLN object. No value found for option: {name}"))?;

    let opt_value = parse_option(name, value).map_err(|e| {
        anyhow!(json!(RpcError {
            code: Some(-32602),
            message: e.to_string(),
            data: None
        }))
    })?;

    let mut config = plugin.state().config.lock();

    check_option(&mut config, name, &opt_value).map_err(|e| {
        anyhow!(json!(RpcError {
            code: Some(-32602),
            message: e.to_string(),
            data: None
        }))
    })?;

    plugin.set_option_str(name, opt_value).map_err(|e| {
        anyhow!(json!(RpcError {
            code: Some(-32602),
            message: e.to_string(),
            data: None
        }))
    })?;

    Ok(json!({}))
}

fn parse_option(name: &str, value: &serde_json::Value) -> Result<options::Value, anyhow::Error> {
    match name {
        n if n.eq(OPT_PAYANY_BUDGET_AMOUNT_MSAT) => {
            if let Some(n_i64) = value.as_i64() {
                return Ok(options::Value::Integer(n_i64));
            } else if let Some(n_str) = value.as_str() {
                if let Ok(n_neg_i64) = n_str.parse::<i64>() {
                    return Ok(options::Value::Integer(n_neg_i64));
                }
            }
            Err(anyhow!("{} is not a valid integer!", n))
        }
        n if n.eq(OPT_PAYANY_HANDLE_PAY) | n.eq(OPT_PAYANY_STRICT_LNURL) => {
            if let Some(n_bool) = value.as_bool() {
                return Ok(options::Value::Boolean(n_bool));
            } else if let Some(n_str) = value.as_str() {
                if let Ok(n_str_bool) = n_str.parse::<bool>() {
                    return Ok(options::Value::Boolean(n_str_bool));
                }
            }
            Err(anyhow!("{} is not a valid boolean!", n))
        }
        _ => {
            if value.is_string() {
                Ok(options::Value::String(value.as_str().unwrap().to_owned()))
            } else {
                Err(anyhow!("{} is not a valid string!", name))
            }
        }
    }
}

fn check_option(
    config: &mut Config,
    name: &str,
    value: &options::Value,
) -> Result<(), anyhow::Error> {
    match name {
        n if n.eq(OPT_PAYANY_BUDGET_AMOUNT_MSAT) => {
            config.budget_amount_msat = Some(Amount::from_msat(options_value_to_u64(
                OPT_PAYANY_BUDGET_AMOUNT_MSAT,
                value.as_i64().unwrap(),
                0,
            )?));
        }
        n if n.eq(OPT_PAYANY_BUDGET_PER) => {
            config.budget_per = Some(parse_time_period(value.as_str().unwrap())?)
        }
        n if n.eq(OPT_PAYANY_HANDLE_PAY) => {
            if config.xpayargs.is_empty() {
                config.xpay_handle_pay = false;
            } else {
                config.xpay_handle_pay = value.as_bool().unwrap()
            }
        }
        n if n.eq(OPT_PAYANY_DNS) => {
            config.dns_server = value
                .as_str()
                .unwrap()
                .parse()
                .map_err(|e| anyhow!("Could not parse DNS server: {}", e))?
        }
        n if n.eq(OPT_PAYANY_STRICT_LNURL) => config.strict_lnurl = value.as_bool().unwrap(),
        _ => return Err(anyhow!("Unknown option: {}", name)),
    }
    Ok(())
}

fn options_value_to_u64(name: &str, value: i64, gteq: u64) -> Result<u64, anyhow::Error> {
    if value >= 0 {
        validate_u64_input(value as u64, name, gteq)
    } else {
        Err(anyhow!(
            "{} needs to be a positive number and not `{}`.",
            name,
            value
        ))
    }
}

fn validate_u64_input(n: u64, var_name: &str, gteq: u64) -> Result<u64, anyhow::Error> {
    if n < gteq {
        return Err(anyhow!(
            "{} must be greater than or equal to {}",
            var_name,
            gteq
        ));
    }

    Ok(n)
}

pub async fn convert_pay_to_xpay(
    plugin: Plugin<PluginState>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), anyhow::Error> {
    let invstring = params.remove("bolt11").unwrap();
    params.insert("invstring".to_owned(), invstring.clone());
    let maxfeepercent = params.remove("maxfeepercent");
    let exemptfee = params.remove("exemptfee");
    let exclude = params.remove("exclude");
    let maxfee = params.get("maxfee").cloned();

    let config = plugin.state().config.lock().clone();

    params.retain(|param, _| config.xpayargs.contains(param));

    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let invoice_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invstring.as_str().unwrap().to_owned(),
        })
        .await?;
    let invoice_amt_msat = match invoice_decoded.item_type {
        cln_rpc::model::responses::DecodeType::BOLT12_INVOICE => {
            invoice_decoded.invoice_amount_msat.unwrap().msat()
        }
        cln_rpc::model::responses::DecodeType::BOLT11_INVOICE => {
            invoice_decoded.amount_msat.unwrap().msat()
        }
        _ => return Err(anyhow!("Wrong invoice type decoded!")),
    };

    if maxfee.is_some() || maxfeepercent.is_some() || exemptfee.is_some() {
        params.insert(
            "maxfee".to_owned(),
            serde_json::Value::Number(
                get_maxfee(maxfee, maxfeepercent, exemptfee, invoice_amt_msat)?.into(),
            ),
        );
    }

    if let Some(excl) = exclude {
        let mut exclude_chans: Vec<ShortChannelIdDir> = Vec::new();
        let mut exclude_nodes: Vec<PublicKey> = Vec::new();
        let exclude_array = excl
            .as_array()
            .ok_or_else(|| anyhow!("exclude is not an array"))?;
        for ex in exclude_array.iter() {
            if let Ok(chan) = serde_json::from_value(ex.clone()) {
                exclude_chans.push(chan);
            } else if let Ok(node) = serde_json::from_value(ex.clone()) {
                exclude_nodes.push(node);
            } else {
                return Err(anyhow!("Could not parse exclude channel/peer:{}", ex));
            }
        }

        _ = rpc
            .call_typed(&AskreneremovelayerRequest {
                layer: invoice_decoded.payment_hash.unwrap().to_string(),
            })
            .await;

        let layers = rpc
            .call_typed(&AskrenecreatelayerRequest {
                persistent: Some(false),
                layer: invoice_decoded.payment_hash.unwrap().to_string(),
            })
            .await?
            .layers;
        let layer = layers.first().unwrap();
        for node in exclude_nodes.into_iter() {
            rpc.call_typed(&AskrenedisablenodeRequest {
                layer: layer.layer.clone(),
                node,
            })
            .await?;
        }
        for chan in exclude_chans.into_iter() {
            rpc.call_typed(&AskreneupdatechannelRequest {
                cltv_expiry_delta: None,
                enabled: Some(false),
                fee_base_msat: None,
                fee_proportional_millionths: None,
                htlc_maximum_msat: None,
                htlc_minimum_msat: None,
                layer: layer.layer.clone(),
                short_channel_id_dir: chan,
            })
            .await?;
        }
        params.insert(
            "layers".to_owned(),
            serde_json::Value::Array(vec![serde_json::Value::String(layer.layer.clone())]),
        );
    }

    Ok(())
}

pub fn get_maxfee(
    maxfee_param: Option<serde_json::Value>,
    maxfeepercent_param: Option<serde_json::Value>,
    exemptfee_param: Option<serde_json::Value>,
    invoice_amount_msat: u64,
) -> Result<u64, anyhow::Error> {
    if maxfee_param.is_some() && (maxfeepercent_param.is_some() || exemptfee_param.is_some()) {
        return Err(anyhow!("Can only set maxfee OR (maxfeepercent/exemptfee)"));
    }
    if let Some(maxfee) = maxfee_param {
        maxfee
            .as_u64()
            .ok_or_else(|| anyhow!("maxfee: should be a millisatoshi amount"))
    } else {
        let maxfee_absolut = if let Some(maxfeep) = maxfeepercent_param {
            let maxfeep_f64 = maxfeep.as_f64().ok_or_else(|| {
                anyhow!(
                    "maxfeepercent: should be a non-negative floating-point number: {}",
                    maxfeep
                )
            })?;
            ((maxfeep_f64 / 100.0) * (invoice_amount_msat as f64)).ceil() as u64
        } else {
            (0.01 * (invoice_amount_msat as f64)).ceil() as u64
        };
        let exemptfee = if let Some(ef) = exemptfee_param {
            ef.as_u64()
                .ok_or_else(|| anyhow!("exemptfee: should be a millisatoshi amount"))?
        } else {
            5000
        };
        Ok(std::cmp::max::<u64>(exemptfee, maxfee_absolut))
    }
}

pub async fn parse_pay_args(plugin: Plugin<PluginState>) -> Result<(), anyhow::Error> {
    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let help_pay = rpc
        .call_typed(&HelpRequest {
            command: Some("pay".to_owned()),
        })
        .await?
        .help;
    let help_xpay = rpc
        .call_typed(&HelpRequest {
            command: Some("xpay".to_owned()),
        })
        .await?
        .help;
    let help_renepay = rpc
        .call_typed(&HelpRequest {
            command: Some("renepay".to_owned()),
        })
        .await?
        .help;

    let mut config = plugin.state().config.lock();
    if let Some(hp) = help_pay.first() {
        for arg in hp.command.split(" ") {
            if arg.eq("pay") {
                continue;
            }
            if arg.starts_with("[") {
                config.payargs.push(arg[1..arg.len() - 1].to_owned());
            } else {
                config.payargs.push(arg.to_owned());
            }
        }
        config.payargs.push("message".to_owned());
    }

    if let Some(hxp) = help_xpay.first() {
        for arg in hxp.command.split(" ") {
            if arg.eq("xpay") {
                continue;
            }
            if arg.starts_with("[") {
                config.xpayargs.push(arg[1..arg.len() - 1].to_owned());
            } else {
                config.xpayargs.push(arg.to_owned());
            }
        }
        config.xpayargs.push("message".to_owned());
    }

    if let Some(hrp) = help_renepay.first() {
        for arg in hrp.command.split(" ") {
            if arg.eq("renepay") {
                continue;
            }
            if arg.starts_with("[") {
                config.renepayargs.push(arg[1..arg.len() - 1].to_owned());
            } else {
                config.renepayargs.push(arg.to_owned());
            }
        }
        config.renepayargs.push("message".to_owned());
    }

    if plugin
        .option_str(OPT_PAYANY_HANDLE_PAY)
        .unwrap()
        .unwrap()
        .as_bool()
        .unwrap()
    {
        config.xpay_handle_pay = !config.xpayargs.is_empty();
    }
    log::debug!("payargs:{}", config.payargs.join(" "));
    log::debug!("xpayargs:{}", config.xpayargs.join(" "));
    log::debug!("renepayargs:{}", config.renepayargs.join(" "));
    Ok(())
}

#[test]
fn test_time_parse() {
    let result = parse_time_period("1sec").unwrap();
    assert_eq!(result, 1);
    let result = parse_time_period("1 s").unwrap();
    assert_eq!(result, 1);
    let result = parse_time_period("3days").unwrap();
    assert_eq!(result, 259200);
    let result = parse_time_period("5h").unwrap();
    assert_eq!(result, 18000);
    let result = parse_time_period("2m").unwrap();
    assert_eq!(result, 120);
    let result = parse_time_period("5w").unwrap();
    assert_eq!(result, 3024000);
    let result = parse_time_period("3    hours").unwrap();
    assert_eq!(result, 10800);
}
