# Math boundaries

Synthetic formulas for native width, source-order copy, and editing checks.

## Long expression

$$a_1+a_2+a_3+a_4+a_5+a_6+a_7+a_8+a_9+a_{10}+a_{11}+a_{12}+a_{13}+a_{14}+a_{15}+a_{16}+a_{17}+a_{18}+a_{19}+a_{20}=S$$

The expression must retain its full-size glyphs and use contained horizontal scrolling when it does not fit.

## Mixed-direction source fallback

النص قبل $x^2 + y^2$ وبعد المعادلة.

This paragraph documents a current limitation: mixed-direction inline formulas keep their canonical source until visual object order and caret mapping are supported.

## Editable formula

$$\frac{37}{41}$$

## Invalid formula

$$\unknownBoundaryCommand{x}$$

The final marker must remain after all formula source in copied text.
