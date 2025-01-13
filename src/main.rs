use anyhow::anyhow;
use cln_plugin::Builder;
use hooks::hook_handler;
use rpc::payany;

mod fetch;
mod hooks;
mod lnurl;
mod offer;
mod rpc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    std::env::set_var("CLN_PLUGIN_LOG", "payany=debug,info");
    log_panics::init();

    let confplugin = match Builder::new(tokio::io::stdin(), tokio::io::stdout())
        .rpcmethod(
            "payany",
            "fetch invoice for static ln payment method",
            payany,
        )
        .hook("rpc_command", hook_handler)
        .dynamic()
        .configure()
        .await?
    {
        Some(plugin) => plugin,
        None => return Err(anyhow!("Error configuring payany!")),
    };
    if let Ok(plugin) = confplugin.start(()).await {
        plugin.join().await
    } else {
        Err(anyhow!("Error starting payany!"))
    }
}
