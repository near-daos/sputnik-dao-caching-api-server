# Sputnik DAO Indexer

This indexer provides endpoints to retrieve proposals from a SputnikDAO smart-contracts with options for filtering, sorting, and different response formats.

## Endpoints

### Get Proposals

```
GET /proposals/<dao_id>?<filters...>
```

Retrieves a list of proposals for a specific DAO with optional filtering and sorting.

#### Path Parameters

- `dao_id` - The account ID of the DAO

#### Query Parameters

**Status Filters:**

- `statuses` - Filter by proposal status (comma-separated values)
  - Values: `Approved`, `Rejected`, `InProgress`, `Expired`, `Removed`, `Moved`, `Failed`
  - Example: `statuses=Approved,Rejected`

**Search Filters:**

- `search` - Filter proposals containing this keyword in description (case-insensitive)
  - Example: `search=payment`

**Proposal Type Filters:**

- `proposal_types` - Filter by proposal types (comma-separated values)
  - Values: `FunctionCall`, `Transfer`, `AddMemberToRole`, `RemoveMemberFromRole`, etc.
  - Example: `proposal_types=FunctionCall,Transfer`

**Proposer Filters:**

- `proposers` - Filter proposals by proposer account(s) (comma-separated, OR logic)
  - Example: `proposers=megha19.near,frol.near`
- `proposers_not` - Exclude proposals by proposer account(s) (comma-separated, NOT logic)
  - Example: `proposers_not=megha19.near,frol.near`

**Approver Filters:**

- `approvers` - Filter proposals approved by account(s) (comma-separated, OR logic)
  - Example: `approvers=megha19.near,frol.near`
- `approvers_not` - Exclude proposals approved by account(s) (comma-separated, NOT logic)
  - Example: `approvers_not=megha19.near,frol.near`

**Voter Vote Filters:**

- `voter_votes` - Filter by specific voter votes (format: "account:vote,account:vote")
  - Vote values: `approved` (Approve vote), `rejected` (Reject/Remove vote)
  - Example: `voter_votes=alice.near:approved,bob.near:rejected`

**Category Filters:**

- `category` - Filter by proposal category
  - Values: `payments`, `lockup`, `asset-exchange`, `stake-delegation`
  - Example: `category=payments`

**Payment-Specific Filters (only apply when category=payments):**

- `recipients` - Filter by payment recipient(s) (comma-separated, OR logic)
  - Example: `recipients=megha19.near,frol.near`
- `recipients_not` - Exclude by payment recipient(s) (comma-separated, NOT logic)
  - Example: `recipients_not=megha19.near,frol.near`
- `tokens` - Filter by token(s) used in payments (comma-separated, OR logic)
  - Values: `near`, `usdt.tether-token.near`, or any token contract ID
  - Note: Empty token strings in proposals are treated as "near"
  - Example: `tokens=near,usdt.tether-token.near`
- `tokens_not` - Exclude by token(s) used in payments (comma-separated, NOT logic)
  - Example: `tokens_not=near`
- `amount_min` - Filter by minimum payment amount (human-readable format)
  - Example: `amount_min=1.5` (1.5 NEAR)
- `amount_max` - Filter by maximum payment amount (human-readable format)
  - Example: `amount_max=10.0` (10.0 NEAR)
- `amount_equal` - Filter by exact payment amount (human-readable format)
  - Example: `amount_equal=5.25` (5.25 NEAR)

**Date Filters:**

- `created_date_from` - Filter proposals created from this date (inclusive)
  - Format: `YYYY-MM-DD` (e.g., `2024-01-15`)
  - Example: `created_date_from=2024-01-15`
- `created_date_to` - Filter proposals created until this date (inclusive)
  - Format: `YYYY-MM-DD` (e.g., `2024-12-31`)
  - Example: `created_date_to=2024-12-31`

**Pagination:**

- `page` - Page number (0-based, default: 0)
  - Example: `page=0`
- `page_size` - Number of proposals per page (default: 50)
  - Example: `page_size=10`

**Sorting:**

- `sort_by` - Sort proposals by field
  - Values: `CreationTime`, `ExpiryTime`
  - Example: `sort_by=CreationTime`
- `sort_direction` - Sort direction
  - Values: `asc` (ascending), `desc` (descending)
  - Example: `sort_direction=desc`

#### Response Format

- JSON (default)

### Get Proposals CSV Export

```
GET /csv/proposals/<dao_id>?<filters...>
```

Retrieves proposals in CSV format with the same filtering options as the JSON endpoint.

#### Response Format

- CSV file download

### Get Specific Proposal

```
GET /proposal/<dao_id>/<proposal_id>
```

Retrieves a specific proposal by ID from a DAO.

#### Path Parameters

- `dao_id` - The account ID of the DAO
- `proposal_id` - The numeric ID of the proposal

### Get DAO Proposers

```
GET /proposals/<dao_id>/proposers
```

Retrieves a list of all unique proposers for a DAO.

### Get DAO Approvers

```
GET /proposals/<dao_id>/approvers
```

Retrieves a list of all unique approvers (voters) for a DAO.

### Get DAO Recipients

```
GET /proposals/<dao_id>/recipients
```

Retrieves a list of all unique payment recipients for a DAO.

### Get DAO Requested Tokens

```
GET /proposals/<dao_id>/requested-tokens
```

Retrieves a list of all unique tokens requested in payment proposals for a DAO.

## Caching

All responses are cached for 5 seconds to improve performance and reduce load on the RPC client. The API fetches the latest data from the cache and applies filters as needed.

### Cache Behavior

- **Cache Duration**: 5 seconds per DAO
- **Cache Hit**: Returns cached data immediately
- **Cache Miss**: Fetches fresh data from NEAR blockchain
- **Cache Persistence**: Cache is persisted to disk and restored on server restart

## Filtering Logic

The filtering system supports complex combinations:

### Multi-Select Filters (OR Logic for Inclusion, NOT Logic for Exclusion)

- **Proposers**: `proposers` (OR), `proposers_not` (NOT)
- **Approvers**: `approvers` (OR), `approvers_not` (NOT)
- **Recipients**: `recipients` (OR), `recipients_not` (NOT)
- **Tokens**: `tokens` (OR), `tokens_not` (NOT)
- **Proposal Types**: `proposal_types` (OR logic)

### Range Filters

- **Amount**: `amount_min`, `amount_max`, `amount_equal` (inclusive ranges, exact match)
- **Dates**: `created_date_from`, `created_date_to` (inclusive date range)

### Special Token Handling

- Empty token strings (`""`) in proposal data are treated as "NEAR" tokens
- Filter input for NEAR should be `"near"` (lowercase)

### Combined Filter Logic

- Different filter types use AND logic (all must match)
- Multi-select filters within the same type use OR logic for inclusion, NOT logic for exclusion
- Amount filters use inclusive ranges (>= min, <= max) or exact match (=)
- Voter vote filters require ALL specified voters to match their expected vote

## Example Curl Requests

### Get All Proposals for a DAO

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near"
```

### Get Approved Payment Proposals

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?statuses=Approved&category=payments"
```

### Get Proposals by Specific Proposer

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?proposers=megha19.near,frol.near"
```

### Get Proposals Excluding Specific Approvers

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?approvers_not=megha19.near,frol.near"
```

### Get Payment Proposals with NEAR Token

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?category=payments&tokens=near"
```

### Get Payment Proposals with Amount Range

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?category=payments&amount_min=1.5&amount_max=10.0"
```

### Get Payment Proposals with Exact Amount

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?category=payments&amount_equal=5.25"
```

### Get Proposals by Date Range

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?created_date_from=2024-01-15&created_date_to=2024-12-31"
```

### Get Proposals by Proposal Type

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?proposal_types=FunctionCall,Transfer"
```

### Get Proposals by Specific Voter Votes

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?voter_votes=alice.near:approved,bob.near:rejected"
```

### Get Proposals with Multiple Filters

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?statuses=Approved&category=payments&proposers=megha19.near&page_size=10"
```

### Get Proposals with Pagination

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near?page=0&page_size=5"
```

### Get All Proposals in CSV Format

```bash
curl -X GET "http://localhost:5001/csv/proposals/testing-astradao.sputnik-dao.near"
```

### Get DAO Proposers

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near/proposers"
```

### Get DAO Requested Tokens

```bash
curl -X GET "http://localhost:5001/proposals/testing-astradao.sputnik-dao.near/requested-tokens"
```

## Response Format Examples

### Proposals Response (JSON)

```json
{
  "proposals": [
    {
      "id": 1,
      "proposer": "megha19.near",
      "description": "Payment proposal for development work",
      "status": "Approved",
      "kind": {
        "Transfer": {
          "receiver_id": "frol.near",
          "amount": "1000000000000000000000000",
          "token_id": ""
        }
      },
      "votes": {
        "megha19.near": "Approve",
        "frol.near": "Approve"
      }
    }
  ],
  "page": 0,
  "page_size": 10,
  "total": 1
}
```

### Requested Tokens Response (JSON)

```json
{
  "requested_tokens": ["near", "usdt.tether-token.near"],
  "total": 2
}
```

### Proposers Response (JSON)

```json
{
  "proposers": ["megha19.near", "frol.near", "alice.near"],
  "total": 3
}
```

### Approvers Response (JSON)

```json
{
  "approvers": ["megha19.near", "frol.near", "bob.near"],
  "total": 3
}
```

### Recipients Response (JSON)

```json
{
  "recipients": ["megha19.near", "frol.near", "charlie.near"],
  "total": 3
}
```

## Testing

1. Initialize the test environment by running:

   ```bash
   ./test_init.sh
   ```

   This script initializes test files and creates a sandbox NEAR state. This process may take some time to complete.

2. Once initialization is complete, run the project tests with:

   ```bash
   ./test.sh
   ```

3. Run specific filter tests:
   ```bash
   cargo test --test filter_test -- --nocapture
   ```

## Error Responses

The API returns standard HTTP status codes:

- **200 OK**: Successful request
- **400 Bad Request**: Invalid parameters (e.g., malformed DAO ID)
- **404 Not Found**: DAO or proposal not found
- **500 Internal Server Error**: Server error

## Development

The project uses Rocket framework for the web server and includes comprehensive test coverage for all filtering functionality. The caching system ensures efficient performance while maintaining data freshness.
