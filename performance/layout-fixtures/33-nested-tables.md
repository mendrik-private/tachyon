# Nested technical tables

Tables retain their measured columns, headers and source order inside authored containers. These are synthetic examples, not inferred cards or captions.

## Quoted configuration

> This configuration table belongs to the quotation.
>
> | Setting | Value |
> | --- | ---: |
> | connection_timeout_seconds | 30 |
> | Maximum workers | 8 |

## List context

- **Deployment check**

  The table belongs to this list item.

  | Environment | Endpoint |
  | --- | --- |
  | Development | localhost |
  | Staging | staging.example.test |

## Notice context

> [!NOTE]
> These values are part of the authored notice.
>
> | Property | Meaning |
> | --- | --- |
> | configuration_schema_revision | Stable identifier |
> | Resource state | Ready |

## Independent comparison

| Choice | Enabled | Explanation |
| --- | ---: | --- |
| Verify | Yes | Preserve all canonical table cells. |
| Preview | Yes | Keep the table in its original container. |

All tables remain in the source, including those outside the current viewport.
