# Inline formulas

Ordinary prose keeps its reading rhythm while formulas share the text baseline.

## Arithmetic and fractions

The identity $E = mc^2$ appears inside a sentence, followed by ordinary text.

For a fraction $\frac{a+b}{c+d}$, the text baseline remains consistent and the next line clears the entire expression. The root $\sqrt{x^2 + y^2}$ is followed by more text to check wrapping.

A probability $P(A \mid B) = \frac{P(B \mid A)P(A)}{P(B)}$ belongs to this sentence. Neither its source nor its rendered expression should disappear when the window becomes narrow.

## Lists and tables

- The area is $\pi r^2$.
- The length is $\sqrt{a^2+b^2}$.
- The series uses $\sum_{i=1}^{n} i$.

| Quantity | Expression |
| :--- | :--- |
| Energy | $E = mc^2$ |
| Average | $\frac{x+y}{2}$ |

## Source editing

Click into this paragraph to edit $x^2 + y^2 = z^2$ as TeX. Click another paragraph to see the formula again. Undo must restore the original Markdown exactly.

An invalid expression $\notARealCommand{x}$ stays accessible as source. A normal inline code span `x^2` is still code, not a formula.

The closing marker confirms source-order copying across every formula.
