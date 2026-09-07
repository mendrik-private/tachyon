# Container boundaries

Tables use equal cell padding. Their enclosing quote or notice owns the space around the table.

## Table-only notice

> [!NOTE]
> | Setting | Value |
> | --- | ---: |
> | Worker count | 8 |
> | Retry count | 3 |

This paragraph follows the notice and must not overlap its border.

## Table-only quotation

> | Name | State |
> | --- | --- |
> | Primary | Ready |
> | Secondary | Waiting |

This paragraph follows the quotation.

## Table before prose

> [!TIP]
> | Check | Result |
> | --- | --- |
> | Source order | Preserved |
>
> This authored explanation remains below its table.

## Table after prose

> [!IMPORTANT]
> This authored explanation remains above its table.
>
> | Check | Result |
> | --- | --- |
> | Stable editing | Required |

The final paragraph remains outside all containers.
