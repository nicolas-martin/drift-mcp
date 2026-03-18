use anyhow::Result;
use std::env;
use std::future::Future;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

const DEFAULT_API_ENDPOINT: &str = "https://data.api.drift.trade";
const DEFAULT_DLOB_ENDPOINT: &str = "https://dlob.drift.trade";

#[tokio::main]
async fn main() -> Result<()> {
    let api_endpoint =
        env::var("DRIFT_API_ENDPOINT").unwrap_or_else(|_| DEFAULT_API_ENDPOINT.to_string());
    let dlob_endpoint =
        env::var("DRIFT_DLOB_ENDPOINT").unwrap_or_else(|_| DEFAULT_DLOB_ENDPOINT.to_string());

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let service = DriftMcp::new(api_endpoint, dlob_endpoint)
        .serve(transport)
        .await?;
    service.waiting().await?;
    Ok(())
}

// ============================================================================
// Tool Request Schemas
// ============================================================================

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetPerpMarketsRequest {}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetSpotMarketsRequest {}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetPerpMarketRequest {
    #[schemars(description = "Market index for the perpetual market (e.g., 0 for SOL-PERP)")]
    market_index: u32,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetSpotMarketRequest {
    #[schemars(description = "Market index for the spot market (e.g., 0 for USDC, 1 for SOL)")]
    market_index: u32,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetFundingRatesRequest {
    #[schemars(description = "Market name for the perpetual market (e.g., 'SOL-PERP', 'BTC-PERP')")]
    market_name: String,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetOrderbookRequest {
    #[schemars(description = "Market name (e.g., 'SOL-PERP' for perp, 'SOL' for spot)")]
    market_name: String,
    #[schemars(description = "Market type: 'perp' or 'spot'")]
    market_type: String,
    #[schemars(description = "Orderbook depth (number of price levels). Default is 10.")]
    depth: Option<u32>,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetUserPositionsRequest {
    #[schemars(description = "User's Solana public key (base58 encoded)")]
    user_public_key: String,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetUserOrdersRequest {
    #[schemars(description = "User's Solana public key (base58 encoded)")]
    user_public_key: String,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetTradeHistoryRequest {
    #[schemars(description = "Market name (e.g., 'SOL-PERP')")]
    market_name: String,
    #[schemars(description = "Number of recent trades to fetch. Default is 100.")]
    limit: Option<u32>,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetOraclePriceRequest {
    #[schemars(description = "Market name (e.g., 'SOL-PERP' or 'SOL')")]
    market_name: String,
    #[schemars(description = "Market type: 'perp' or 'spot'")]
    market_type: String,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetFundingRateHistoryRequest {
    #[schemars(description = "Market name for the perpetual market (e.g., 'SOL-PERP')")]
    market_name: String,
    #[schemars(description = "Number of records to fetch. Default is 24 (1 day of hourly rates).")]
    limit: Option<u32>,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetContractsRequest {}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetUserPnlRequest {
    #[schemars(description = "User's Solana public key (base58 encoded)")]
    user_public_key: String,
    #[schemars(description = "Whether to include unsettled funding payments in the PnL calculation. Default is false.")]
    with_funding: Option<bool>,
    #[schemars(description = "Market index to calculate PnL for a specific market. If not provided, calculates for all markets.")]
    market_index: Option<u32>,
}

#[derive(schemars::JsonSchema, Deserialize)]
pub struct GetUserFundingPnlRequest {
    #[schemars(description = "User's Solana public key (base58 encoded)")]
    user_public_key: String,
    #[schemars(description = "Market index to get funding PnL for a specific market. If not provided, returns total funding PnL.")]
    market_index: Option<u32>,
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerpMarket {
    pub market_index: u32,
    pub symbol: String,
    #[serde(default)]
    pub base_asset_symbol: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotMarket {
    pub market_index: u32,
    pub symbol: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    pub slot: u64,
    pub funding_rate: String,
    pub oracle_price_twap: String,
    pub mark_price_twap: String,
    #[serde(default)]
    pub funding_rate_long: Option<String>,
    #[serde(default)]
    pub funding_rate_short: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FundingRatesResponse {
    #[serde(rename = "fundingRates")]
    pub funding_rates: Vec<FundingRate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderbookLevel {
    pub price: String,
    pub size: String,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderbookResponse {
    pub bids: Vec<OrderbookLevel>,
    pub asks: Vec<OrderbookLevel>,
    #[serde(default)]
    pub slot: Option<u64>,
    #[serde(default)]
    pub oracle: Option<f64>,
}

// ============================================================================
// Main Server Implementation
// ============================================================================

pub struct DriftMcp {
    tool_router: ToolRouter<DriftMcp>,
    http_client: reqwest::Client,
    api_endpoint: String,
    dlob_endpoint: String,
}

#[tool_router]
impl DriftMcp {
    fn new(api_endpoint: String, dlob_endpoint: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            http_client: reqwest::Client::new(),
            api_endpoint,
            dlob_endpoint,
        }
    }

    // ========================================================================
    // Tools
    // ========================================================================

    #[tool(description = "Get all available perpetual markets on Drift Protocol")]
    async fn get_perp_markets(
        &self,
        Parameters(_req): Parameters<GetPerpMarketsRequest>,
    ) -> String {
        let url = format!("{}/perpMarkets", self.api_endpoint);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching perp markets: {}", e),
        }
    }

    #[tool(description = "Get all available spot markets on Drift Protocol")]
    async fn get_spot_markets(
        &self,
        Parameters(_req): Parameters<GetSpotMarketsRequest>,
    ) -> String {
        let url = format!("{}/spotMarkets", self.api_endpoint);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching spot markets: {}", e),
        }
    }

    #[tool(description = "Get details for a specific perpetual market by index")]
    async fn get_perp_market(
        &self,
        Parameters(req): Parameters<GetPerpMarketRequest>,
    ) -> String {
        let url = format!("{}/perpMarkets?marketIndex={}", self.api_endpoint, req.market_index);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching perp market: {}", e),
        }
    }

    #[tool(description = "Get details for a specific spot market by index")]
    async fn get_spot_market(
        &self,
        Parameters(req): Parameters<GetSpotMarketRequest>,
    ) -> String {
        let url = format!("{}/spotMarkets?marketIndex={}", self.api_endpoint, req.market_index);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching spot market: {}", e),
        }
    }

    #[tool(description = "Get funding rates for a perpetual market. Returns historical funding rate data including hourly rates and APR calculations.")]
    async fn get_funding_rates(
        &self,
        Parameters(req): Parameters<GetFundingRatesRequest>,
    ) -> String {
        let url = format!("{}/fundingRates?marketName={}", self.api_endpoint, req.market_name);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<FundingRatesResponse>().await {
                Ok(data) => {
                    if data.funding_rates.is_empty() {
                        return format!("No funding rates found for market: {}", req.market_name);
                    }
                    
                    // Calculate and format the latest funding rate
                    let latest = &data.funding_rates[data.funding_rates.len() - 1];
                    let funding_rate: f64 = latest.funding_rate.parse().unwrap_or(0.0);
                    let oracle_twap: f64 = latest.oracle_price_twap.parse().unwrap_or(1.0);
                    
                    // Convert to percentage (funding_rate is in 1e9, oracle_twap is in 1e6)
                    let funding_rate_pct = (funding_rate / 1e9) / (oracle_twap / 1e6);
                    let funding_rate_apr = funding_rate_pct * 24.0 * 365.0 * 100.0;
                    
                    let mut result = format!(
                        "Funding Rates for {}:\n\n",
                        req.market_name
                    );
                    result.push_str(&format!(
                        "Latest Funding Rate: {:.9}%/hour ({:.2}% APR)\n",
                        funding_rate_pct * 100.0,
                        funding_rate_apr
                    ));
                    result.push_str(&format!(
                        "Oracle TWAP: ${:.2}\n",
                        oracle_twap / 1e6
                    ));
                    result.push_str(&format!(
                        "Total Records: {}\n\n",
                        data.funding_rates.len()
                    ));
                    
                    // Show last 5 funding rates
                    result.push_str("Recent Funding Rates:\n");
                    for rate in data.funding_rates.iter().rev().take(5) {
                        let fr: f64 = rate.funding_rate.parse().unwrap_or(0.0);
                        let ot: f64 = rate.oracle_price_twap.parse().unwrap_or(1.0);
                        let pct = (fr / 1e9) / (ot / 1e6) * 100.0;
                        result.push_str(&format!(
                            "  Slot {}: {:.9}%/hour\n",
                            rate.slot, pct
                        ));
                    }
                    
                    result
                }
                Err(e) => format!("Error parsing funding rates: {}", e),
            },
            Err(e) => format!("Error fetching funding rates: {}", e),
        }
    }

    #[tool(description = "Get the orderbook for a market. Returns bids and asks with price and size.")]
    async fn get_orderbook(
        &self,
        Parameters(req): Parameters<GetOrderbookRequest>,
    ) -> String {
        let depth = req.depth.unwrap_or(10);
        let market_type = req.market_type.to_lowercase();
        
        let url = format!(
            "{}/l2?marketName={}&marketType={}&depth={}",
            self.dlob_endpoint, req.market_name, market_type, depth
        );
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<OrderbookResponse>().await {
                Ok(data) => {
                    let mut result = format!(
                        "Orderbook for {} ({}):\n\n",
                        req.market_name, req.market_type
                    );
                    
                    if let Some(oracle) = data.oracle {
                        result.push_str(&format!("Oracle Price: ${:.4}\n\n", oracle));
                    }
                    
                    result.push_str("ASKS (Sell Orders):\n");
                    for (i, ask) in data.asks.iter().take(depth as usize).enumerate() {
                        result.push_str(&format!(
                            "  {}. ${} @ {} size\n",
                            i + 1, ask.price, ask.size
                        ));
                    }
                    
                    result.push_str("\nBIDS (Buy Orders):\n");
                    for (i, bid) in data.bids.iter().take(depth as usize).enumerate() {
                        result.push_str(&format!(
                            "  {}. ${} @ {} size\n",
                            i + 1, bid.price, bid.size
                        ));
                    }
                    
                    result
                }
                Err(e) => format!("Error parsing orderbook: {}", e),
            },
            Err(e) => format!("Error fetching orderbook: {}", e),
        }
    }

    #[tool(description = "Get positions for a user by their Solana public key")]
    async fn get_user_positions(
        &self,
        Parameters(req): Parameters<GetUserPositionsRequest>,
    ) -> String {
        let url = format!(
            "{}/userPositions?userPubKey={}",
            self.api_endpoint, req.user_public_key
        );
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching user positions: {}", e),
        }
    }

    #[tool(description = "Get open orders for a user by their Solana public key")]
    async fn get_user_orders(
        &self,
        Parameters(req): Parameters<GetUserOrdersRequest>,
    ) -> String {
        let url = format!(
            "{}/userOrders?userPubKey={}",
            self.api_endpoint, req.user_public_key
        );
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching user orders: {}", e),
        }
    }

    #[tool(description = "Get recent trade history for a market")]
    async fn get_trade_history(
        &self,
        Parameters(req): Parameters<GetTradeHistoryRequest>,
    ) -> String {
        let limit = req.limit.unwrap_or(100);
        let url = format!(
            "{}/trades?marketName={}&limit={}",
            self.api_endpoint, req.market_name, limit
        );
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching trade history: {}", e),
        }
    }

    #[tool(description = "Get the current oracle price for a market")]
    async fn get_oracle_price(
        &self,
        Parameters(req): Parameters<GetOraclePriceRequest>,
    ) -> String {
        let market_type = req.market_type.to_lowercase();
        let url = format!(
            "{}/l2?marketName={}&marketType={}&depth=1",
            self.dlob_endpoint, req.market_name, market_type
        );
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<OrderbookResponse>().await {
                Ok(data) => {
                    if let Some(oracle) = data.oracle {
                        format!(
                            "Oracle Price for {} ({}): ${:.6}",
                            req.market_name, req.market_type, oracle
                        )
                    } else {
                        format!("Oracle price not available for {}", req.market_name)
                    }
                }
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching oracle price: {}", e),
        }
    }

    #[tool(description = "Get historical funding rate data for a perpetual market with calculated APR")]
    async fn get_funding_rate_history(
        &self,
        Parameters(req): Parameters<GetFundingRateHistoryRequest>,
    ) -> String {
        let url = format!("{}/fundingRates?marketName={}", self.api_endpoint, req.market_name);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<FundingRatesResponse>().await {
                Ok(data) => {
                    if data.funding_rates.is_empty() {
                        return format!("No funding rate history found for market: {}", req.market_name);
                    }
                    
                    let limit = req.limit.unwrap_or(24) as usize;
                    let mut result = format!(
                        "Funding Rate History for {} (last {} records):\n\n",
                        req.market_name, limit.min(data.funding_rates.len())
                    );
                    
                    result.push_str("Slot | Hourly Rate | APR\n");
                    result.push_str("-".repeat(50).as_str());
                    result.push_str("\n");
                    
                    for rate in data.funding_rates.iter().rev().take(limit) {
                        let fr: f64 = rate.funding_rate.parse().unwrap_or(0.0);
                        let ot: f64 = rate.oracle_price_twap.parse().unwrap_or(1.0);
                        let pct = (fr / 1e9) / (ot / 1e6);
                        let apr = pct * 24.0 * 365.0 * 100.0;
                        result.push_str(&format!(
                            "{} | {:.9}% | {:.2}%\n",
                            rate.slot, pct * 100.0, apr
                        ));
                    }
                    
                    result
                }
                Err(e) => format!("Error parsing funding rate history: {}", e),
            },
            Err(e) => format!("Error fetching funding rate history: {}", e),
        }
    }

    #[tool(description = "Get contract information for all perpetual markets including funding rates, open interest, and 24h volume")]
    async fn get_contracts(
        &self,
        Parameters(_req): Parameters<GetContractsRequest>,
    ) -> String {
        let url = format!("{}/contracts", self.api_endpoint);
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching contracts: {}", e),
        }
    }

    #[tool(description = "Get unrealized PnL for a user. Can optionally include funding payments and filter by market.")]
    async fn get_user_pnl(
        &self,
        Parameters(req): Parameters<GetUserPnlRequest>,
    ) -> String {
        let mut url = format!(
            "{}/v2/unrealizedPNL?userPubKey={}",
            self.api_endpoint, req.user_public_key
        );
        
        if let Some(with_funding) = req.with_funding {
            url.push_str(&format!("&withFunding={}", with_funding));
        }
        
        if let Some(market_index) = req.market_index {
            url.push_str(&format!("&marketIndex={}", market_index));
        }
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching user PnL: {}", e),
        }
    }

    #[tool(description = "Get unrealized funding PnL for a user. Returns the accumulated funding payments that haven't been settled yet.")]
    async fn get_user_funding_pnl(
        &self,
        Parameters(req): Parameters<GetUserFundingPnlRequest>,
    ) -> String {
        let mut url = format!(
            "{}/user/unrealized_funding_pnl?userPubKey={}",
            self.api_endpoint, req.user_public_key
        );
        
        if let Some(market_index) = req.market_index {
            url.push_str(&format!("&marketIndex={}", market_index));
        }
        
        match self.http_client.get(&url).send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error serializing response: {}", e)),
                Err(e) => format!("Error parsing response: {}", e),
            },
            Err(e) => format!("Error fetching user funding PnL: {}", e),
        }
    }
}

// ============================================================================
// Server Handler with prompts
// ============================================================================

#[tool_handler]
impl ServerHandler for DriftMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
            server_info: Implementation {
                name: "Drift Protocol MCP".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(
                "Drift Protocol MCP server providing tools for querying perpetual and spot markets, \
                 funding rates, orderbooks, positions, and trading data on the Drift decentralized exchange."
                    .to_string(),
            ),
        }
    }

    // ========================================================================
    // Prompts
    // ========================================================================

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        async move {
            Ok(ListPromptsResult {
                prompts: vec![
                    Prompt {
                        name: "compare-funding-rates".to_string(),
                        description: Some(
                            "Compare funding rates across multiple perpetual markets to find arbitrage opportunities".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "markets".to_string(),
                            description: Some("Comma-separated list of markets (e.g., 'SOL-PERP,BTC-PERP,ETH-PERP')".to_string()),
                            required: Some(true),
                        }]),
                    },
                    Prompt {
                        name: "analyze-market-depth".to_string(),
                        description: Some(
                            "Analyze the orderbook depth and liquidity for a specific market".to_string(),
                        ),
                        arguments: Some(vec![
                            PromptArgument {
                                name: "market_name".to_string(),
                                description: Some("Market name (e.g., 'SOL-PERP')".to_string()),
                                required: Some(true),
                            },
                            PromptArgument {
                                name: "market_type".to_string(),
                                description: Some("Market type: 'perp' or 'spot'".to_string()),
                                required: Some(true),
                            },
                        ]),
                    },
                    Prompt {
                        name: "check-user-portfolio".to_string(),
                        description: Some(
                            "Get a comprehensive overview of a user's positions and open orders".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "user_public_key".to_string(),
                            description: Some("User's Solana public key".to_string()),
                            required: Some(true),
                        }]),
                    },
                    Prompt {
                        name: "funding-rate-alert".to_string(),
                        description: Some(
                            "Check if funding rate exceeds a threshold for a market".to_string(),
                        ),
                        arguments: Some(vec![
                            PromptArgument {
                                name: "market_name".to_string(),
                                description: Some("Market name (e.g., 'SOL-PERP')".to_string()),
                                required: Some(true),
                            },
                            PromptArgument {
                                name: "threshold_apr".to_string(),
                                description: Some("APR threshold percentage (e.g., '50' for 50% APR)".to_string()),
                                required: Some(true),
                            },
                        ]),
                    },
                    Prompt {
                        name: "market-overview".to_string(),
                        description: Some(
                            "Get a complete overview of a market including price, funding, and orderbook".to_string(),
                        ),
                        arguments: Some(vec![
                            PromptArgument {
                                name: "market_name".to_string(),
                                description: Some("Market name (e.g., 'SOL-PERP')".to_string()),
                                required: Some(true),
                            },
                        ]),
                    },
                    Prompt {
                        name: "list-all-markets".to_string(),
                        description: Some(
                            "List all available perpetual and spot markets on Drift".to_string(),
                        ),
                        arguments: None,
                    },
                    Prompt {
                        name: "markets-summary".to_string(),
                        description: Some(
                            "Get a summary of all perpetual markets with funding rates, open interest, and volume".to_string(),
                        ),
                        arguments: None,
                    },
                    Prompt {
                        name: "user-pnl-summary".to_string(),
                        description: Some(
                            "Get a user's unrealized PnL including funding payments".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "user_public_key".to_string(),
                            description: Some("User's Solana public key".to_string()),
                            required: Some(true),
                        }]),
                    },
                    Prompt {
                        name: "user-funding-analysis".to_string(),
                        description: Some(
                            "Analyze a user's accumulated funding payments across all positions".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "user_public_key".to_string(),
                            description: Some("User's Solana public key".to_string()),
                            required: Some(true),
                        }]),
                    },
                ],
                next_cursor: None,
            })
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResult, ErrorData>> + Send + '_ {
        async move {
            let args = request.arguments.unwrap_or_default();

            match request.name.as_str() {
                "compare-funding-rates" => {
                    let markets = args.get("markets").and_then(|v| v.as_str()).unwrap_or("SOL-PERP,BTC-PERP,ETH-PERP");
                    Ok(GetPromptResult {
                        description: Some(
                            "Compare funding rates across multiple perpetual markets".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Fetch the funding rates for the following markets: {}. \
                                 Compare them and identify which markets have the highest and lowest funding rates. \
                                 Calculate the APR for each and highlight any significant arbitrage opportunities \
                                 (where funding rate differences could be exploited).",
                                markets
                            ),
                        )],
                    })
                }
                "analyze-market-depth" => {
                    let market_name = args.get("market_name").and_then(|v| v.as_str()).unwrap_or("SOL-PERP");
                    let market_type = args.get("market_type").and_then(|v| v.as_str()).unwrap_or("perp");
                    Ok(GetPromptResult {
                        description: Some(
                            "Analyze orderbook depth and liquidity".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Fetch the orderbook for {} ({} market) with depth of 20 levels. \
                                 Analyze the bid-ask spread, total liquidity on each side, \
                                 identify any significant support/resistance levels, \
                                 and assess the market's overall liquidity quality.",
                                market_name, market_type
                            ),
                        )],
                    })
                }
                "check-user-portfolio" => {
                    let user_public_key = args.get("user_public_key").and_then(|v| v.as_str()).unwrap_or("");
                    Ok(GetPromptResult {
                        description: Some(
                            "Get comprehensive user portfolio overview".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Fetch all positions and open orders for the user with public key: {}. \
                                 Provide a summary of their portfolio including: \
                                 1) All perp positions (long/short, size, unrealized PnL if available) \
                                 2) All spot holdings \
                                 3) Any open orders and their status \
                                 4) Overall portfolio risk assessment",
                                user_public_key
                            ),
                        )],
                    })
                }
                "funding-rate-alert" => {
                    let market_name = args.get("market_name").and_then(|v| v.as_str()).unwrap_or("SOL-PERP");
                    let threshold = args.get("threshold_apr").and_then(|v| v.as_str()).unwrap_or("50");
                    Ok(GetPromptResult {
                        description: Some(
                            "Check funding rate against threshold".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Check the current funding rate for {}. \
                                 Alert if the annualized funding rate (APR) exceeds {}%. \
                                 If it does, explain the implications for long and short positions \
                                 and suggest potential trading strategies.",
                                market_name, threshold
                            ),
                        )],
                    })
                }
                "market-overview" => {
                    let market_name = args.get("market_name").and_then(|v| v.as_str()).unwrap_or("SOL-PERP");
                    Ok(GetPromptResult {
                        description: Some(
                            "Complete market overview".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Provide a complete overview of the {} market including: \
                                 1) Current oracle price \
                                 2) Current funding rate and APR \
                                 3) Top 5 bids and asks from the orderbook \
                                 4) Bid-ask spread analysis \
                                 5) Recent funding rate trend (last few hours)",
                                market_name
                            ),
                        )],
                    })
                }
                "list-all-markets" => {
                    Ok(GetPromptResult {
                        description: Some(
                            "List all available markets on Drift".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            "Fetch and display all available perpetual markets and spot markets on Drift. \
                             Organize them by category and include their market indices for reference."
                                .to_string(),
                        )],
                    })
                }
                "markets-summary" => {
                    Ok(GetPromptResult {
                        description: Some(
                            "Summary of all perpetual markets".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            "Fetch contract information for all perpetual markets on Drift. \
                             For each market, show the funding rate (converted to APR), open interest, and 24h volume. \
                             Rank the markets by open interest and highlight any markets with unusually high or low funding rates."
                                .to_string(),
                        )],
                    })
                }
                "user-pnl-summary" => {
                    let user_public_key = args.get("user_public_key").and_then(|v| v.as_str()).unwrap_or("");
                    Ok(GetPromptResult {
                        description: Some(
                            "User unrealized PnL summary".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Fetch the unrealized PnL for user {} including funding payments. \
                                 Also fetch their current positions to provide context. \
                                 Summarize their total unrealized PnL, break it down by position if possible, \
                                 and explain whether they are currently profitable or not.",
                                user_public_key
                            ),
                        )],
                    })
                }
                "user-funding-analysis" => {
                    let user_public_key = args.get("user_public_key").and_then(|v| v.as_str()).unwrap_or("");
                    Ok(GetPromptResult {
                        description: Some(
                            "User funding payment analysis".to_string(),
                        ),
                        messages: vec![PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Fetch the unrealized funding PnL for user {}. \
                                 Also fetch their current positions to understand which positions are generating funding payments. \
                                 Explain whether they are net paying or receiving funding, \
                                 and provide recommendations on funding rate exposure.",
                                user_public_key
                            ),
                        )],
                    })
                }
                _ => Err(ErrorData::invalid_params(
                    format!("Unknown prompt: {}", request.name),
                    None,
                )),
            }
        }
    }
}
