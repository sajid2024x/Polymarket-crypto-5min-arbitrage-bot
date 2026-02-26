use anyhow::Result;
use dashmap::DashMap;
use futures::{Stream, StreamExt};
use polymarket_client_sdk::clob::ws::{
    Client as WsClient,
    types::response::BookUpdate,
};
use polymarket_client_sdk::types::{B256, U256};
use std::collections::HashMap;
use std::pin::Pin;
use tracing::{debug, info};

use crate::market::MarketInfo;

/// 缩短 B256 用于日志
#[inline]
fn short_b256(b: &B256) -> String {
    let s = format!("{b}");
    if s.len() > 12 { format!("{}..", &s[..10]) } else { s }
}

/// 缩短 U256 用于日志
#[inline]
fn short_u256(u: &U256) -> String {
    let s = format!("{u}");
    if s.len() > 12 {
        format!("..{}", &s[s.len().saturating_sub(8)..])
    } else {
        s
    }
}

pub struct OrderBookMonitor {
    ws_client: WsClient,
    books: DashMap<U256, BookUpdate>,
    market_map: HashMap<B256, (U256, U256)>, // market_id -> (yes, no)
}

pub struct OrderBookPair {
    pub yes_book: BookUpdate,
    pub no_book: BookUpdate,
    pub market_id: B256,
}

impl OrderBookMonitor {
    pub fn new() -> Self {
        Self {
            ws_client: WsClient::default(),
            books: DashMap::new(),
            market_map: HashMap::new(),
        }
    }

    pub fn subscribe_market(&mut self, market: &MarketInfo) -> Result<()> {
        self.market_map.insert(
            market.market_id,
            (market.yes_token_id, market.no_token_id),
        );

        info!(
            market_id = short_b256(&market.market_id),
            yes = short_u256(&market.yes_token_id),
            no = short_u256(&market.no_token_id),
            "订阅市场订单簿"
        );

        Ok(())
    }

    pub fn create_orderbook_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<BookUpdate>> + Send + '_>>> {
        let token_ids: Vec<U256> = self
            .market_map
            .values()
            .flat_map(|(y, n)| [*y, *n])
            .collect();

        if token_ids.is_empty() {
            return Err(anyhow::anyhow!("没有市场需要订阅"));
        }

        info!(token_count = token_ids.len(), "创建订单簿订阅流");

        let stream = self.ws_client.subscribe_orderbook(token_ids)?;
        Ok(Box::pin(stream.map(|r| r.map_err(|e| anyhow::anyhow!("{e}")))))
    }

    /// ❗ READ-ONLY — NO MUTATION
    pub fn handle_book_update(&self, book: BookUpdate) -> Option<OrderBookPair> {
        self.books.insert(book.asset_id, book.clone());

        for (market_id, (yes, no)) in &self.market_map {
            if book.asset_id == *yes {
                if let Some(no_book) = self.books.get(no) {
                    return Some(OrderBookPair {
                        yes_book: book.clone(),
                        no_book: no_book.clone(),
                        market_id: *market_id,
                    });
                }
            } else if book.asset_id == *no {
                if let Some(yes_book) = self.books.get(yes) {
                    return Some(OrderBookPair {
                        yes_book: yes_book.clone(),
                        no_book: book.clone(),
                        market_id: *market_id,
                    });
                }
            }
        }
        None
    }

    pub fn get_book(&self, token_id: U256) -> Option<BookUpdate> {
        self.books.get(&token_id).map(|b| b.clone())
    }

    pub fn clear(&mut self) {
        self.books.clear();
        self.market_map.clear();
    }
}
