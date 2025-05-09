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
- `status` - Filter by proposal status
- `keyword` - Filter proposals containing this keyword in description (case-insensitive)
- `proposer` - Filter proposals by proposer account
- `proposal_type` - Filter by proposal type(s), with support for advanced JSON path filtering
- `min_votes` - Filter proposals with at least this many votes
- `approvers` - Filter proposals approved by all listed accounts
- `sort_by` - Sort proposals by either `creation_time` or `expiry_time`
- `sort_direction` - Sort direction: `asc` (default) or `desc`

#### Response Format
- JSON (default)
- CSV (set `Accept: text/csv` header)

### Get Specific Proposal

```
GET /proposals/<dao_id>/<proposal_id>
```

Retrieves a specific proposal by ID from a DAO.

#### Path Parameters
- `dao_id` - The account ID of the DAO
- `proposal_id` - The numeric ID of the proposal

#### Response Format
- JSON (default)
- CSV (set `Accept: text/csv` header)

## Caching

All responses are cached for 5 seconds to improve performance and reduce load on the RPC client. The API fetches the latest data from the cache and applies filters as needed.

## Filtering Logic

The filtering system allows for combinations of filters:
- All filter categories must be satisfied (AND logic between different filter types)
- It is possible to have several `proposal_type` filters, a proposal must match all one of the specified types
- It is possible to have several  `approvers`, a proposal must be approved by all specified accounts (AND logic)

## Proposal Type Filtering

The API supports advanced filtering based on proposal types and their properties using a JSON path notation with comparison operators.

### Format

```
<json_path>[:<operator><value>]
```

Where:
- `json_path` is a colon-separated path into the proposal's `kind` JSON structure
- `operator` can be:
  - `=` for equality comparison
  - `>` for less than
  - `<` for greater than

### Examples

1. Filter by basic proposal type:
```
proposal_type=FunctionCall
```

2. Filter by proposal type with specific receiver:
```
proposal_type=Transfer:receiver_id=app.near
```

3. Filter transfers with amount less than 1 NEAR:
```
proposal_type=Transfer:amount>1000000000000000000000000
```

4. Filter by multiple criteria:
```
proposal_type=Transfer:receiver_id=app.near&proposal_type=Transfer:amount>1000000000000000000000000
```


## Example Curl Requests

### Get All Proposals for a DAO

```bash
curl -X GET "http://example.com/proposals/mydao.near"
```

### Get Proposals Approved by Specific Accounts, Sorted by Creation Time Descending

```bash
curl -X GET "http://example.com/proposals/mydao.near?approvers=user1.near&approvers=user2.near&sort_by=CreationTime&sort_direction=desc"
```

### Get a Specific Proposal in CSV Format

```bash
curl -X GET "http://example.com/proposals/mydao.near/42" -H "Accept: text/csv"
```

### Get All Proposals in CSV Format

```bash
curl -X GET "http://example.com/proposals/mydao.near" -H "Accept: text/csv"
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
