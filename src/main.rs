use rustls::crypto::ring;

#[ctor::ctor]
fn install_rustls_provider() {
    ring::default_provider()
        .install_default()
        .expect("failed to install rustls ring provider");
}

mod config;
mod market;
mod monitor;
mod risk;
mod trading;
mod utils;
mod scalp;

use anyhow::Result;
use futures::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use crate::scalp::ScalpState;

use polymarket_client_sdk::types::{Address, B256, U256};

use crate::config::Config;
use crate::market::{MarketDiscoverer, MarketInfo, MarketScheduler};
use crate::monitor::{ArbitrageDetector, OrderBookMonitor};
use crate::risk::{RiskManager, PositionBalancer, HedgeMonitor};
use crate::trading::TradingExecutor;

#[tokio::main]
async fn main() -> Result<()> {
    utils::logger::init_logger()?;
    info!("🚀 bot starting");

    let config = Config::from_env()?;
    info!("config loaded");

    // ===== SCALPING STATE (NEW) =====
    let mut monitor = OrderBookMonitor::new();

    let mut scalp = ScalpState::new();

    let discoverer = MarketDiscoverer::new(config.crypto_symbols.clone());
    let scheduler = MarketScheduler::new(discoverer, config.market_refresh_advance_secs);
    let detector = ArbitrageDetector::new(config.min_profit_threshold);

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

    executor.verify_authentication().await?;
    info!("auth verified");

    let risk_manager = Arc::new(RiskManager::new_from_config(&config).await?);

    let position_balancer = Arc::new(PositionBalancer::new(
        risk_manager.clone(),
        &config,
    ));

    let wind_down_in_progress = Arc::new(AtomicBool::new(false));

    loop {
        let markets = scheduler.get_markets_immediately_or_wait().await?;
        if markets.is_empty() {
            warn!("no markets");
            sleep(Duration::from_secs(30)).await;
            continue;
        }

        risk_manager.position_tracker().reset_exposure();

        let mut monitor = OrderBookMonitor::new();
        for m in &markets {
            monitor.subscribe_market(m)?;
        }

        let mut stream = monitor.create_orderbook_stream()?;
        info!("📡 monitoring orderbooks");
        let mut scalp = ScalpState::new();

        loop {
            tokio::select! {
                msg = stream.next() => {
                    match msg {
                        Some(Ok(book)) => {
                            if let Some(pair) = monitor.handle_book_update(book) {
                                scalp.detect(
    pair.market_id,
    &pair.yes_book,
    dec!(0.002), // 0.2% move
);

                                // ===== SCALPING SIGNAL (NEW) =====
                                if config.enable_scalping {
                                    let threshold = dec!(0.003); // 0.3%
                                    scalp.detect(
                                        pair.market_id,
                                        &pair.yes_book,
                                        threshold,
                                    );
                                }

                                // ===== EXISTING ARBITRAGE LOGIC =====
                                if let Some(opp) = detector.check_arbitrage(
                                    &pair.yes_book,
                                    &pair.no_book,
                                    &pair.market_id,
                                ) {
                                    let total_price = opp.yes_ask_price + opp.no_ask_price;
                                    let exec_threshold =
                                        dec!(1.0) - Decimal::from_f64(config.arbitrage_execution_spread).unwrap();

                                    if total_price <= exec_threshold {
                                        info!(
                                            "🚨 arbitrage | market={:?} profit={:.2}%",
                                            pair.market_id,
                                            opp.profit_percentage
                                        );

                                        let exec = executor.clone();
                                        let rm = risk_manager.clone();
                                        tokio::spawn(async move {
                                            if let Ok(result) =
                                                exec.execute_arbitrage_pair(&opp, "", "").await
                                            {
                                                rm.register_order_pair(
                                                    result,
                                                    opp.market_id,
                                                    opp.yes_token_id,
                                                    opp.no_token_id,
                                                    opp.yes_ask_price,
                                                    opp.no_ask_price,
                                                );
                                            }
                                        });
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!("stream error: {}", e);
                            break;
                        }
                        None => {
                            warn!("stream ended");
                            break;
                        }
                    }
                }

                _ = sleep(Duration::from_secs(1)) => {}
            }
        }
    }
}
