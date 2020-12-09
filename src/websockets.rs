use crate::model::*;
use crate::errors::*;
use url::Url;
use serde_json::from_value;

use std::sync::atomic::{AtomicBool, Ordering};
use tungstenite::{connect, Message};
use tungstenite::protocol::WebSocket;
use tungstenite::client::AutoStream;
use tungstenite::handshake::client::Response;

static WEBSOCKET_URL: &str = "wss://fstream.binance.com/stream?streams=";

static OUTBOUND_ACCOUNT_INFO: &str = "outboundAccountInfo";
static EXECUTION_REPORT: &str = "executionReport";

static KLINE: &str = "kline";
static AGGREGATED_TRADE: &str = "aggTrade";
static DEPTH_ORDERBOOK: &str = "depthUpdate";
static PARTIAL_ORDERBOOK: &str = "lastUpdateId";

static DAYTICKER: &str = "24hrTicker";

static ACCOUNT_UPDATE: &str = "ACCOUNT_UPDATE";
static ORDER_TRADE_UPDATE: &str = "ORDER_TRADE_UPDATE";

#[allow(clippy::large_enum_variant)]
pub enum WebsocketEvent {
    AccountUpdate(AccountUpdateEvent),
    OrderTrade(OrderTradeEvent),
    Trade(TradesEvent),
    OrderBook(OrderBook),
    DayTicker(Vec<DayTickerEvent>),
    Kline(KlineEvent),
    DepthOrderBook(DepthOrderBookEvent),
    BookTicker(BookTickerEvent),
    FuturesAccountUpdateEvent(FuturesAccountUpdateEvent),
    OrderTradeUpdateEvent(OrderTradeUpdateEvent),
    FuturesFunding(FuturesFunding),
}

pub struct WebSockets<'a> {
    pub socket: Option<(WebSocket<AutoStream>, Response)>,
    handler: Box<dyn FnMut(WebsocketEvent) -> Result<()> + 'a>,
}

impl<'a> WebSockets<'a> {
    pub fn new<Callback>(handler: Callback) -> WebSockets<'a>
    where
        Callback: FnMut(WebsocketEvent) -> Result<()> + 'a,
    {
        WebSockets {
            socket: None,
            handler: Box::new(handler),
        }
    }

    pub fn connect(&mut self, endpoint: &str) -> Result<()> {
        let wss: String = format!("{}{}", WEBSOCKET_URL, endpoint);
        let url = Url::parse(&wss)?;

        match connect(url) {
            Ok(answer) => {
                self.socket = Some(answer);
                Ok(())
            }
            Err(e) => {
                bail!(format!("Error during handshake {}", e));
            }
        }
    }

    pub fn disconnect(&mut self) -> Result<()> {
        if let Some(ref mut socket) = self.socket {
            socket.0.close(None)?;
            Ok(())
        } else {
            bail!("Not able to close the connection");
        }
    }

    pub fn event_loop(&mut self, running: &AtomicBool) -> Result<()> {
        while running.load(Ordering::Relaxed) {
            if let Some(ref mut socket) = self.socket {
                let message = socket.0.read_message()?;

                match message {
                    Message::Text(msg) => {
                        let mut stream_val: serde_json::Value = serde_json::from_str(&msg)?;
                        match &stream_val["stream"] {
                            serde_json::Value::String(stream_name) => {
                                if stream_val["data"].is_object() {
                                    if stream_name.contains("markPrice") {
                                        let futures_funding: FuturesFunding = from_value(stream_val["data"].take()).unwrap();
                                        (self.handler)(WebsocketEvent::FuturesFunding(futures_funding))?;
                                    }
                                    else {
                                        let stream_data = stream_val["data"].as_object().unwrap();
                                        if stream_data.get("u") != None &&
                                            stream_data.get("s") != None &&
                                            stream_data.get("b") != None &&
                                            stream_data.get("B") != None &&
                                            stream_data.get("a") != None &&
                                            stream_data.get("A") != None
                                        {
                                            let book_ticker: BookTickerEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::BookTicker(book_ticker))?;
                                        } else if msg.find(OUTBOUND_ACCOUNT_INFO) != None {
                                            let account_update: AccountUpdateEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::AccountUpdate(account_update))?;
                                        } else if msg.find(EXECUTION_REPORT) != None {
                                            let order_trade: OrderTradeEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::OrderTrade(order_trade))?;
                                        } else if msg.find(AGGREGATED_TRADE) != None {
                                            let trade: TradesEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::Trade(trade))?;
                                        } else if msg.find(DAYTICKER) != None {
                                            let trades: Vec<DayTickerEvent> = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::DayTicker(trades))?;
                                        } else if msg.find(KLINE) != None {
                                            let kline: KlineEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::Kline(kline))?;
                                        } else if msg.find(PARTIAL_ORDERBOOK) != None {
                                            let partial_orderbook: OrderBook = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::OrderBook(partial_orderbook))?;
                                        } else if msg.find(DEPTH_ORDERBOOK) != None {
                                            let depth_orderbook: DepthOrderBookEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::DepthOrderBook(depth_orderbook))?;
                                        } else if msg.find(ACCOUNT_UPDATE) != None {
                                            let futures_account_update: FuturesAccountUpdateEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::FuturesAccountUpdateEvent(futures_account_update))?;
                                        } else if msg.find(ORDER_TRADE_UPDATE) != None {
                                            let order_trade_update: OrderTradeUpdateEvent = from_value(stream_val["data"].take()).unwrap();
                                            (self.handler)(WebsocketEvent::OrderTradeUpdateEvent(order_trade_update))?;
                                        }
                                    }
                                }
                            },
                            _ => (),
                        };
                    }
                    Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {}
                    Message::Close(e) => {
                        bail!(format!("Disconnected {:?}", e));
                    }
                }
            }
        }
        Ok(())
    }
}
