# drift-mcp

A Drift Protocol MCP server that provides tools for querying the Drift decentralized exchange on Solana.

## Tools

### Market Data
- `get_perp_markets` - Get all available perpetual markets
- `get_spot_markets` - Get all available spot markets
- `get_perp_market` - Get details for a specific perpetual market by index
- `get_spot_market` - Get details for a specific spot market by index
- `get_contracts` - Get contract info for all perp markets (funding rates, open interest, 24h volume)

### Pricing & Funding
- `get_oracle_price` - Get the current oracle price for a market
- `get_funding_rates` - Get current funding rate with APR calculation
- `get_funding_rate_history` - Get historical funding rates with APR

### Orderbook
- `get_orderbook` - Get orderbook bids and asks for any market

### User Data
- `get_user_positions` - Get all positions for a user by public key
- `get_user_orders` - Get open orders for a user by public key
- `get_user_pnl` - Get unrealized PnL for a user (optionally including funding)
- `get_user_funding_pnl` - Get unrealized funding PnL for a user

### Trading History
- `get_trade_history` - Get recent trade history for a market

## Prompts

- `compare-funding-rates` - Compare funding rates across multiple markets for arbitrage opportunities
- `analyze-market-depth` - Analyze orderbook depth and liquidity
- `check-user-portfolio` - Get comprehensive portfolio overview for a user
- `funding-rate-alert` - Check if funding rate exceeds a threshold
- `market-overview` - Get complete market overview (price, funding, orderbook)
- `list-all-markets` - List all available markets on Drift
- `markets-summary` - Get summary of all perp markets with funding rates, open interest, and volume
- `user-pnl-summary` - Get a user's unrealized PnL including funding payments
- `user-funding-analysis` - Analyze a user's accumulated funding payments across positions

## Usage

```bash
cargo build --release
```

## Configuration

Add to your MCP config file:

```json
{
  "mcpServers": {
    "drift-mcp": {
      "command": "/path/to/drift-mcp/target/release/drift-mcp"
    }
  }
}
```
