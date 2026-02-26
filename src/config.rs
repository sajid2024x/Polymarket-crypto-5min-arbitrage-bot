use anyhow::Result;
use polymarket_client_sdk::clob::types::OrderType;
use polymarket_client_sdk::types::Address;
use std::env;

/* ============================================================
   env helpers (YOU WERE MISSING THESE)
   ============================================================ */

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

/* ============================================================
   parsers
   ============================================================ */

fn parse_arbitrage_order_type(s: &str) -> OrderType {
    match s.trim().to_uppercase().as_str() {
        "GTC" => OrderType::GTC,
        "GTD" => OrderType::GTD,
        "FOK" => OrderType::FOK,
        "FAK" => OrderType::FAK,
        _ => OrderType::GTD,
    }
}

fn parse_slippage(s: &str) -> [f64; 2] {
    let parts: Vec<f64> = s
        .split(',')
        .map(|x| x.trim().parse().unwrap_or(0.0))
        .collect();

    match parts.len() {
        0 => [0.0, 0.01],
        1 => [parts[0], parts[0]],
        _ => [parts[0], parts[1]],
    }
}

/* ============================================================
   Config struct
   ============================================================ */

#[derive(Debug, Clone)]
pub struct Config {
    pub private_key: String,
    pub proxy_address: Option<Address>,

    pub min_profit_threshold: f64,
    pub max_order_size_usdc: f64,
    pub crypto_symbols: Vec<String>,
    pub market_refresh_advance_secs: u64,

    pub risk_max_exposure_usdc: f64,
    pub risk_imbalance_threshold: f64,

    pub hedge_take_profit_pct: f64,
    pub hedge_stop_loss_pct: f64,

    pub arbitrage_execution_spread: f64,
    pub slippage: [f64; 2],

    pub gtd_expiration_secs: u64,
    pub arbitrage_order_type: OrderType,
    pub stop_arbitrage_before_end_minutes: u64,

    pub merge_interval_minutes: u64,

    pub min_yes_price_threshold: f64,
    pub min_no_price_threshold: f64,

    pub position_sync_interval_secs: u64,
    pub position_balance_interval_secs: u64,
    pub position_balance_threshold: f64,
    pub position_balance_min_total: f64,

    pub wind_down_before_window_end_minutes: u64,
    pub wind_down_sell_price: f64,

    // ===== scalping =====
    pub enable_scalping: bool,
    pub scalp_order_size_usdc: f64,
    pub scalp_take_profit_pct: f64,
    pub scalp_stop_loss_pct: f64,
    pub scalp_max_hold_seconds: u64,

    pub max_trades_per_day: u32,
}

/* ============================================================
   from_env
   ============================================================ */

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let proxy_address: Option<Address> = env::var("POLYMARKET_PROXY_ADDRESS")
            .ok()
            .and_then(|v| v.parse().ok());

        Ok(Self {
            private_key: env::var("POLYMARKET_PRIVATE_KEY")
                .expect("POLYMARKET_PRIVATE_KEY must be set"),

            proxy_address,

            min_profit_threshold: env_f64("MIN_PROFIT_THRESHOLD", 0.001),
            max_order_size_usdc: env_f64("MAX_ORDER_SIZE_USDC", 100.0),

            crypto_symbols: env::var("CRYPTO_SYMBOLS")
                .unwrap_or_else(|_| "btc,eth,xrp,sol".to_string())
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect(),

            market_refresh_advance_secs: env_u64("MARKET_REFRESH_ADVANCE_SECS", 5),

            risk_max_exposure_usdc: env_f64("RISK_MAX_EXPOSURE_USDC", 1000.0),
            risk_imbalance_threshold: env_f64("RISK_IMBALANCE_THRESHOLD", 0.1),

            hedge_take_profit_pct: env_f64("HEDGE_TAKE_PROFIT_PCT", 0.05),
            hedge_stop_loss_pct: env_f64("HEDGE_STOP_LOSS_PCT", 0.05),

            arbitrage_execution_spread: env_f64("ARBITRAGE_EXECUTION_SPREAD", 0.01),

            slippage: parse_slippage(
                &env::var("SLIPPAGE").unwrap_or_else(|_| "0,0.01".to_string()),
            ),

            gtd_expiration_secs: env_u64("GTD_EXPIRATION_SECS", 300),

            arbitrage_order_type: parse_arbitrage_order_type(
                &env::var("ARBITRAGE_ORDER_TYPE").unwrap_or_else(|_| "GTD".to_string()),
            ),

            stop_arbitrage_before_end_minutes: env_u64(
                "STOP_ARBITRAGE_BEFORE_END_MINUTES",
                0,
            ),

            merge_interval_minutes: env_u64("MERGE_INTERVAL_MINUTES", 0),

            min_yes_price_threshold: env_f64("MIN_YES_PRICE_THRESHOLD", 0.0),
            min_no_price_threshold: env_f64("MIN_NO_PRICE_THRESHOLD", 0.0),

            position_sync_interval_secs: env_u64("POSITION_SYNC_INTERVAL_SECS", 10),
            position_balance_interval_secs: env_u64(
                "POSITION_BALANCE_INTERVAL_SECS",
                60,
            ),
            position_balance_threshold: env_f64(
                "POSITION_BALANCE_THRESHOLD",
                2.0,
            ),
            position_balance_min_total: env_f64(
                "POSITION_BALANCE_MIN_TOTAL",
                5.0,
            ),

            wind_down_before_window_end_minutes: env_u64(
                "WIND_DOWN_BEFORE_WINDOW_END_MINUTES",
                0,
            ),
            wind_down_sell_price: env_f64("WIND_DOWN_SELL_PRICE", 0.01),

            // ===== scalping =====
            enable_scalping: env_bool("ENABLE_SCALPING", false),
            scalp_order_size_usdc: env_f64("SCALP_ORDER_SIZE_USDC", 1.0),
            scalp_take_profit_pct: env_f64("SCALP_TAKE_PROFIT_PCT", 1.0),
            scalp_stop_loss_pct: env_f64("SCALP_STOP_LOSS_PCT", 0.5),
            scalp_max_hold_seconds: env_u64("SCALP_MAX_HOLD_SECONDS", 90),

            max_trades_per_day: env_u32("MAX_TRADES_PER_DAY", 5),
        })
    }
}
