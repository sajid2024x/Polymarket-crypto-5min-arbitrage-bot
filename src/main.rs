mod config;
mod market;
mod monitor;
mod risk;
mod trading;
mod utils;

use anyhow::Result;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

use crate::config::Config;
use crate::market::{MarketDiscoverer, MarketScheduler};
use crate::monitor::{ArbitrageDetector, OrderBookMonitor};
use crate::risk::positions::PositionTracker;
use crate::risk::{HedgeMonitor, PositionBalancer, RiskManager};
use crate::trading::TradingExecutor;

#[tokio::main]
async fn main() -> Result<()> {
    // 🔐 FIX rustls crypto provider (this solved your panic)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    // logger
    utils::logger::init_logger()?;
    info!("Polymarket 5分钟套利机器人启动");

    // license check
    poly_5min_bot::trial::check_license()?;

    // load config
    let config = Config::from_env()?;
    info!("配置加载完成");

    // kill switch
    let kill_switch = Arc::new(AtomicBool::new(config.kill_switch));

    // ===== market discovery =====
    let discoverer = MarketDiscoverer::new(config.crypto_symbols.clone());
    let scheduler = MarketScheduler::new(discoverer, config.market_refresh_advance_secs);

    // ===== arbitrage detector =====
    let detector = ArbitrageDetector::new(config.min_profit_threshold);

    // ===== trading executor =====
    info!("初始化交易执行器...");
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

    // ===== risk manager =====
    info!("初始化风险管理模块...");
    let risk_manager = Arc::new(RiskManager::new_from_private_key(
        &config.private_key,
        &config,
    ).await?);

    let position_tracker = Arc::new(PositionTracker::new());
    let balancer = PositionBalancer::new(
        executor.clone(),
        risk_manager.clone(),
        position_tracker.clone(),
        config.max_order_size_usdc,
    );

    let hedge_monitor = HedgeMonitor::new(
        executor.clone(),
        risk_manager.clone(),
        position_tracker.clone(),
    );

    let orderbook_monitor = OrderBookMonitor::new();

    info!("✅ 所有组件初始化完成，进入交易主循环");

    // ===== REAL TRADING LOOP =====
    loop {
        // kill switch check
        if kill_switch.load(Ordering::Relaxed) {
            info!("🛑 kill switch ON → trading paused");
            sleep(Duration::from_secs(5)).await;
            continue;
        }

        // 1️⃣ get active markets
        let markets = scheduler.get_current_markets().await?;
        if markets.is_empty() {
            sleep(Duration::from_secs(2)).await;
            continue;
        }

        // 2️⃣ subscribe orderbooks
        orderbook_monitor.subscribe(markets.clone()).await?;

        // 3️⃣ detect arbitrage
        if let Some(signal) = detector.detect(&orderbook_monitor).await {
            info!("🚨 arbitrage opportunity detected");

            // 4️⃣ risk check
            if risk_manager.allow_trade(&signal).await {
                executor.execute(signal).await?;
            }
        }

        // 5️⃣ hedge & rebalance
        hedge_monitor.check().await?;
        balancer.rebalance().await?;

        sleep(Duration::from_secs(1)).await;
    }
}
