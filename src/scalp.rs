use std::collections::HashMap;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use polymarket_client_sdk::types::B256;
use polymarket_client_sdk::clob::ws::types::response::BookUpdate;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ScalpPosition {
    pub entry_price: Decimal,
    pub size_usdc: Decimal,
    pub opened_at: Instant,
}

#[derive(Debug)]
pub struct ScalpState {
    /// market_id -> last mid price (for signal detection)
    last_mid_price: HashMap<B256, Decimal>,

    /// market_id -> open scalp position
    positions: HashMap<B256, ScalpPosition>,

    /// trade counter (simple version)
    trades_today: u32,
}

impl ScalpState {
    pub fn new() -> Self {
        Self {
            last_mid_price: HashMap::new(),
            positions: HashMap::new(),
            trades_today: 0,
        }
    }

    fn mid_price(book: &BookUpdate) -> Option<Decimal> {
        let bid = book.bids.first()?.price;
        let ask = book.asks.first()?.price;
        Some((bid + ask) / Decimal::from(2))
    }

    /// STEP 1: detect price movement (signal only)
    pub fn detect_signal(
        &mut self,
        market_id: B256,
        yes_book: &BookUpdate,
        threshold_pct: Decimal,
    ) -> bool {
        let mid = match Self::mid_price(yes_book) {
            Some(m) => m,
            None => return false,
        };

        let mut signal = false;

        if let Some(last) = self.last_mid_price.get(&market_id) {
            let diff = (mid - *last) / *last;

            if diff.abs() >= threshold_pct {
                info!(
                    market_id = ?market_id,
                    last = %last,
                    now = %mid,
                    move_pct = %diff,
                    "📈 SCALP SIGNAL"
                );
                signal = true;
            }
        }

        self.last_mid_price.insert(market_id, mid);
        signal
    }

    /// can we open a new scalp?
    pub fn can_open_trade(&self, max_trades_per_day: u32) -> bool {
        self.trades_today < max_trades_per_day
    }

    /// open a scalp position
    pub fn open_position(
        &mut self,
        market_id: B256,
        entry_price: Decimal,
        size_usdc: Decimal,
    ) {
        self.positions.insert(
            market_id,
            ScalpPosition {
                entry_price,
                size_usdc,
                opened_at: Instant::now(),
            },
        );
        self.trades_today += 1;

        info!(
            market_id = ?market_id,
            entry = %entry_price,
            size = %size_usdc,
            "🟢 SCALP OPENED"
        );
    }

    /// close scalp
    pub fn close_position(&mut self, market_id: &B256, reason: &str) {
        if let Some(pos) = self.positions.remove(market_id) {
            info!(
                market_id = ?market_id,
                entry = %pos.entry_price,
                held_secs = pos.opened_at.elapsed().as_secs(),
                reason = reason,
                "🔴 SCALP CLOSED"
            );
        }
    }

    pub fn get_position(&self, market_id: &B256) -> Option<&ScalpPosition> {
        self.positions.get(market_id)
    }

    pub fn is_expired(&self, market_id: &B256, max_hold: Duration) -> bool {
        self.positions
            .get(market_id)
            .map(|p| p.opened_at.elapsed() >= max_hold)
            .unwrap_or(false)
    }
}
