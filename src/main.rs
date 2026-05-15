use std::path::Path;

use anyhow::anyhow;
use cln_plugin::{
    Builder,
    HookBuilder,
    HookFilter,
    RpcMethodBuilder,
    options::{DefaultBooleanConfigOption, IntegerConfigOption, StringConfigOption},
};
use cln_rpc::{
    ClnRpc,
    model::requests::{GetinfoRequest, ListconfigsRequest},
};
use hooks::hook_handler;
use parse::{get_startup_options, parse_pay_args, setconfig_callback};
use rpc::payany;
use structs::PluginState;
use util::check_handle_option;

use crate::util::at_or_above_version;

mod budget;
mod fetch;
mod hooks;
mod lnurl;
mod parse;
mod rpc;
mod structs;
mod util;

const OPT_PAYANY_BUDGET_PER: &str = "payany-budget-per";
const OPT_PAYANY_BUDGET_AMOUNT_MSAT: &str = "payany-budget-amount-msat";
const OPT_PAYANY_HANDLE_PAY: &str = "payany-xpay-handle-pay";
const OPT_PAYANY_STRICT_LNURL: &str = "payany-strict-lnurl";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    unsafe { std::env::set_var("CLN_PLUGIN_LOG", "payany=trace,info") };
    log_panics::init();
    let _ = rustls::crypto::ring::default_provider().install_default();

    let state = PluginState::default();

    let opt_payany_budget_per = StringConfigOption::new_str_no_default(
        OPT_PAYANY_BUDGET_PER,
        "rolling time interval for the budget",
    )
    .dynamic();
    let opt_payany_budget_amount_msat = IntegerConfigOption::new_i64_no_default(
        OPT_PAYANY_BUDGET_AMOUNT_MSAT,
        "budget in msat allowed to be spent in time interval",
    )
    .dynamic();
    let opt_payany_handle_pay = DefaultBooleanConfigOption::new_bool_with_default(
        OPT_PAYANY_HANDLE_PAY,
        false,
        "payany handles conversion of pay to xpay",
    )
    .dynamic();
    let opt_payany_strict_lnurl = DefaultBooleanConfigOption::new_bool_with_default(
        OPT_PAYANY_STRICT_LNURL,
        false,
        "payany adheres strictly to lud-06 and lud-16",
    )
    .dynamic();

    let confplugin = match Builder::new(tokio::io::stdin(), tokio::io::stdout())
        .option(opt_payany_budget_per)
        .option(opt_payany_budget_amount_msat)
        .option(opt_payany_handle_pay)
        .option(opt_payany_strict_lnurl)
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("payany", payany)
                .description("fetch invoice for static ln payment method")
                .usage("invstring amount_msat [message]"),
        )
        .hook_from_builder(HookBuilder::new("rpc_command", hook_handler).filters(vec![
            HookFilter::Str("xpay".to_owned()),
            HookFilter::Str("pay".to_owned()),
            HookFilter::Str("renepay".to_owned()),
            HookFilter::Str("setconfig".to_owned()),
        ]))
        .setconfig_callback(setconfig_callback)
        .dynamic()
        .configure()
        .await?
    {
        Some(plugin) => {
            match get_startup_options(&plugin, &state) {
                Ok(()) => &(),
                Err(e) => return plugin.disable(format!("{e}").as_str()).await,
            };
            log::debug!("read startup options done");

            plugin
        }
        None => return Err(anyhow!("Error configuring payany!")),
    };
    match confplugin.start(state).await {
        Ok(plugin) => {
            {
                let mut rpc = ClnRpc::new(
                    Path::new(&plugin.configuration().lightning_dir)
                        .join(plugin.configuration().rpc_file),
                )
                .await?;

                let cln_version = rpc.call_typed(&GetinfoRequest {}).await?.version;

                let listconfigs = rpc
                    .call_typed(&ListconfigsRequest { config: None })
                    .await?
                    .configs
                    .ok_or_else(|| anyhow!("No `configs` found in listconfigs response"))?;

                let mut config = plugin.state().config.lock();
                config.version = cln_version;

                config.tor_proxy = if let Some(proxy_config) = listconfigs.proxy {
                    if let Some(always_use_proxy_config) = listconfigs.always_use_proxy {
                        if always_use_proxy_config.value_bool {
                            log::info!("Using tor proxy: {}", proxy_config.value_str);
                            Some(proxy_config.value_str)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let allow_deprecated_apis =
                    if let Some(deprecated_apis_config) = listconfigs.allow_deprecated_apis {
                        deprecated_apis_config.value_bool
                    } else {
                        true
                    };

                config.ignore_deprecated_pays = (at_or_above_version(&config.version, "26.06")?
                    && !allow_deprecated_apis)
                    || at_or_above_version(&config.version, "27.03")?;
            }
            match parse_pay_args(plugin.clone()).await {
                Ok(()) => (),
                Err(e) => {
                    println!(
                        "{}",
                        serde_json::json!({"jsonrpc": "2.0",
                                    "method": "log",
                                    "params": {"level":"warn",
                                    "message":format!("Error parsing pay args: {}", e)}})
                    );
                    return Err(e);
                }
            }
            match check_handle_option(plugin.clone()).await {
                Ok(()) => (),
                Err(e) => log::info!("{e}"),
            }
            log::debug!("ready");
            plugin.join().await
        }
        _ => Err(anyhow!("Error starting payany!")),
    }
}
