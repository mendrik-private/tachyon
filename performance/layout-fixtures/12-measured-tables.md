# Tables with measured widths

This synthetic document checks intrinsic column sizing, adjacent table arrangements, source order and narrow-screen overflow.

## Configuration

### Runtime properties

| Property | Value |
| :--- | ---: |
| Timeout | 30 s |
| Workers | 4 |
| Cache | 64 MiB |

### Environment comparison

| Environment | Service endpoint | Retries |
| :--- | :--- | ---: |
| Development | https://development.example.test/service | 3 |
| Staging | https://staging.example.test/service | 2 |
| Production | https://production.example.test/service | 5 |

## Explanation and table

Use these defaults for a local worker.

| Setting | Default | Purpose |
| :--- | ---: | :--- |
| Workers | 4 | Limit concurrent jobs |
| Timeout | 30 s | Bound waiting time |
| Retries | 3 | Retry transient failures |

This paragraph must start below the whole arrangement, not inside a table or beside its last row.

## Unbroken identifiers

| Identifier | Description |
| :--- | :--- |
| configuration_schema_revision_identifier | Keep this identifier complete and horizontally accessible on a narrow viewport. |
| id | Compact labels should not receive the same minimum width as long identifiers. |
