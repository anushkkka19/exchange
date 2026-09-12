use std::collections::BTreeMap;

use uuid::Uuid;

struct Order {
    pub order_id: Uuid,
    pub user_id: Uuid,
    pub qty: u32,
    pub filled_qty: u32,
}

struct Orderbook {
    pub symbol: String,
    pub asks: BTreeMap<u32, Order>,
    pub bids: BTreeMap<u32, Order>  
}   

impl Orderbook {
    fn new(&mut self, symbol: String){
        self.symbol = symbol;
        self.asks = BTreeMap::new();
        self.bids = BTreeMap::new();
    }
}
