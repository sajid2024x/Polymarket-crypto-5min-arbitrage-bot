mod config;
mod market;
mod monitor;
mod risk;
mod trading;
mod utils;

use anyhow::Result;
use tracing::info;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // 🔐 rustls crypto provider fix (required for Railway)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls ring provider");

    // logger
    utils::logger::init_logger()?;
    info!("Polymarket 5分钟套利机器人启动");

    // license
    poly_5min_bot::trial::check_license()?;

    // load config
    let config = Config::from_env()?;
    info!("配置加载完成");

    // 🔑 private key validation (already expected by repo)
    config.validate_private_key()?;
    info!("私钥格式验证通过");

    // 🚀 START BOT
    //
    // IMPORTANT:
    // This repo’s trading loop lives inside `trading::run()`
    // main.rs is only responsible for booting it
    //
    trading::run(config).await?;

    Ok(())
}
