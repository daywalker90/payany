use std::path::Path;

use anyhow::anyhow;
use cln_plugin::{
    options::{
        DefaultBooleanConfigOption, DefaultStringConfigOption, IntegerConfigOption,
        StringConfigOption,
    },
    Builder,
};
use cln_rpc::{model::requests::GetinfoRequest, ClnRpc};
use hooks::hook_handler;
use parse::{get_startup_options, parse_pay_args, setconfig_callback};
use rpc::payany;
use structs::PluginState;
use util::check_handle_option;

mod budget;
mod fetch;
mod hooks;
mod lnurl;
mod offer;
mod parse;
mod rpc;
mod structs;
mod util;

const OPT_PAYANY_BUDGET_PER: &str = "payany-budget-per";
const OPT_PAYANY_BUDGET_AMOUNT_MSAT: &str = "payany-budget-amount-msat";
const OPT_PAYANY_HANDLE_PAY: &str = "payany-xpay-handle-pay";
const OPT_PAYANY_DNS: &str = "payany-dns";
const OPT_PAYANY_STRICT_LNURL: &str = "payany-strict-lnurl";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    std::env::set_var("CLN_PLUGIN_LOG", "payany=debug,info");
    log_panics::init();

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
    let opt_payany_dns = DefaultStringConfigOption::new_str_with_default(
        OPT_PAYANY_DNS,
        "google",
        "DNS server to be used for bip353 lookups",
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
        .option(opt_payany_dns)
        .option(opt_payany_strict_lnurl)
        .rpcmethod(
            "payany",
            "fetch invoice for static ln payment method",
            payany,
        )
        .hook("rpc_command", hook_handler)
        .setconfig_callback(setconfig_callback)
        .dynamic()
        .configure()
        .await?
    {
        Some(plugin) => {
            match get_startup_options(&plugin, state.clone()) {
                Ok(()) => &(),
                Err(e) => return plugin.disable(format!("{}", e).as_str()).await,
            };
            log::debug!("read startup options done");

            plugin
        }
        None => return Err(anyhow!("Error configuring payany!")),
    };
    if let Ok(plugin) = confplugin.start(state).await {
        {
            let mut rpc = ClnRpc::new(
                Path::new(&plugin.configuration().lightning_dir)
                    .join(plugin.configuration().rpc_file),
            )
            .await?;
            plugin.state().config.lock().version =
                rpc.call_typed(&GetinfoRequest {}).await?.version;
        }
        match parse_pay_args(plugin.clone()).await {
            Ok(_) => (),
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
        };
        match check_handle_option(plugin.clone()).await {
            Ok(()) => (),
            Err(e) => log::info!("{}", e),
        };
        log::debug!("ready");
        plugin.join().await
    } else {
        Err(anyhow!("Error starting payany!"))
    }
}
