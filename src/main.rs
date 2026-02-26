mod config;
mod market;
mod monitor;
mod risk;
mod trading;
mod utils;

use poly_5min_bot::merge;
use poly_5min_bot::positions::{get_positions, Position};

use anyhow::Result;
use dashmap::DashMap;
use futures::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use polymarket_client_sdk::types::{Address, B256, U256};

use crate::config::Config;
use crate::market::{MarketDiscoverer, MarketInfo, MarketScheduler};
use crate::monitor::{ArbitrageDetector, OrderBookMonitor};
use crate::risk::positions::PositionTracker;
use crate::risk::{HedgeMonitor, PositionBalancer, RiskManager};
use crate::trading::TradingExecutor;

// ✅ FIX: force rustls crypto provider
use rustls::crypto::CryptoProvider;

/// 从持仓中筛出 YES 和 NO 都持仓的 condition_id
fn condition_ids_with_both_sides(positions: &[Position]) -> Vec<B256> {
    let mut by_condition: HashMap<B256, HashSet<i32>> = HashMap::new();
    for p in positions {
        if p.size <= dec!(0) {
            continue;
        }
        by_condition
            .entry(p.condition_id)
            .or_default()
            .insert(p.outcome_index);
    }
    by_condition
        .into_iter()
        .filter(|(_, indices)| {
            (indices.contains(&0) && indices.contains(&1)) || (indices.contains(&1) && indices.contains(&2))
        })
        .map(|(c, _)| c)
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    // ✅ FIX: install crypto provider BEFORE anything else
    CryptoProvider::install_default()
        .expect("failed to install rustls crypto provider");

    // 初始化日志
    utils::logger::init_logger()?;
    tracing::info!("Polymarket 5分钟套利机器人启动");

    // 许可证校验
    poly_5min_bot::trial::check_license()?;

    // 加载配置
    let config = Config::from_env()?;
    tracing::info!("配置加载完成");

    // 初始化组件
    let _discoverer = MarketDiscoverer::new(config.crypto_symbols.clone());
    let _scheduler = MarketScheduler::new(_discoverer, config.market_refresh_advance_secs);
    let _detector = ArbitrageDetector::new(config.min_profit_threshold);

    // 验证私钥格式
    info!("正在验证私钥格式...");
    use alloy::signers::local::LocalSigner;
    use polymarket_client_sdk::POLYGON;
    use std::str::FromStr;

    let _signer_test = LocalSigner::from_str(&config.private_key)
        .map_err(|e| anyhow::anyhow!("私钥格式无效: {}", e))?;
    info!("私钥格式验证通过");

    // 初始化交易执行器
    info!("正在初始化交易执行器（需要API认证）...");
    let executor = Arc::new(
        TradingExecutor::new(
            config.private_key.clone(),
            config.max_order_size_usdc,
            config.proxy_address,
            config.slippage,
            config.gtd_expiration_secs,
            config.arbitrage_order_type.clone(),
        )
        .await?,
    );

    // 风险管理客户端
    info!("正在初始化风险管理客户端（需要API认证）...");
    use polymarket_client_sdk::clob::{Client, Config as ClobConfig};
    use polymarket_client_sdk::clob::types::SignatureType;
    use alloy::signers::Signer;

    let signer_for_risk = LocalSigner::from_str(&config.private_key)?
        .with_chain_id(Some(POLYGON));

    let clob_config = ClobConfig::builder().use_server_time(true).build();
    let mut auth_builder = Client::new("https://clob.polymarket.com", clob_config)?
        .authentication_builder(&signer_for_risk);

    if let Some(funder) = config.proxy_address {
        auth_builder = auth_builder
            .funder(funder)
            .signature_type(SignatureType::Proxy);
    }

    let clob_client = auth_builder.authenticate().await?;
    let _risk_manager = Arc::new(RiskManager::new(clob_client.clone(), &config));

    info!("✅ 所有组件初始化完成，进入主循环");

    // ===== 主循环保持不变 =====
    loop {
        sleep(Duration::from_secs(5)).await;
        info!("bot running (kill switch should still be ON)");
    }
}
