use std::collections::HashMap;
use rust_decimal::Decimal;
use polymarket_client_sdk::types::B256;
use polymarket_client_sdk::clob::ws::types::response::BookUpdate;
use tracing::info;

pub struct ScalpState {
    last_mid_price: HashMap<B256, Decimal>,
}

impl ScalpState {
    pub fn new() -> Self {
        Self {
            last_mid_price: HashMap::new(),
        }
    }

    fn mid_price(book: &BookUpdate) -> Option<Decimal> {
        let bid = book.bids.first()?.price;
        let ask = book.asks.first()?.price;
        Some((bid + ask) / Decimal::from(2))
    }

    pub fn detect(
        &mut self,
        market_id: B256,
        yes_book: &BookUpdate,
        threshold_pct: Decimal,
    ) {
        let mid = match Self::mid_price(yes_book) {
            Some(m) => m,
            None => return,
        };

        if let Some(last) = self.last_mid_price.get(&market_id) {
            let diff = (mid - *last) / *last;

            if diff.abs() >= threshold_pct {
                info!(
                    "📈 scalp signal | market={:?} last={} now={} move={}",
                    market_id, last, mid, diff
                );
            }
        }

        self.last_mid_price.insert(market_id, mid);
    }
}
