# Display mathematics

Synthetic source-preservation fixture. The equations use ordinary double-dollar delimiters.

## Quadratic roots

$$x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}$$

## Matrix alignment

$$\begin{pmatrix}a & b \\ c & d\end{pmatrix}$$

## Multiline formula source

$$
\int_0^1 x^2\,dx = \frac{1}{3}
$$

## Inline source remains meaningful

The source $x^2 + \alpha$ is an inline formula, not Markdown emphasis.

## Nested source

> $$E = mc^2$$

## Malformed input remains editable

$$\frac{unfinished$$
