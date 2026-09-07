"""Reconstruct equations from native AT-SPI tags/children as Orca does."""
import xml.etree.ElementTree as ET


def mathml(nodes, root):
    def element(index):
        node = nodes[index]
        tag = node.get('attributes', {}).get('tag')
        if tag not in {'math', 'mrow', 'mi', 'mn', 'mo', 'mtext', 'mfrac', 'msqrt',
                       'mroot', 'msub', 'msup', 'msubsup', 'munder', 'mover',
                       'munderover', 'mphantom', 'mtable', 'mtr', 'mtd'}:
            raise RuntimeError(f'Missing or unsupported native math tag: {tag!r}')
        result = ET.Element(tag)
        if tag in {'mi', 'mn', 'mo', 'mtext'}:
            result.text = node['name']
        else:
            for child, record in enumerate(nodes):
                if record['parent'] == index:
                    result.append(element(child))
        return result
    return element(root)


def check(nodes):
    expected = {
        r'\frac{37}{41}': ('mfrac', ['37', '41']),
        r'\sqrt[3]{x_2^4}': ('mroot', ['x', '2', '4', '3']),
        r'\begin{pmatrix}17&23\\31&43\end{pmatrix}': ('mtable', ['(', '17', '23', '31', '43', ')']),
        r'\sum_{i=1}^{9}i^2': ('munderover', ['∑', 'i', '=', '1', '9', 'i', '2']),
    }
    equations = {}
    for source, (structure, tokens) in expected.items():
        matches = [index for index, node in enumerate(nodes) if node['role'] == 'math' and node['name'] == source + '\n']
        if len(matches) != 1:
            raise RuntimeError(f'Formula source must have one canonical math location: {source}; found {[n["name"] for n in nodes if n["role"] == "math"]!r}')
        tree = mathml(nodes, matches[0])
        actual = [node.text for node in tree.iter() if node.tag in {'mi', 'mn', 'mo', 'mtext'}]
        if tree.find(f'.//{structure}') is None or actual != tokens:
            raise RuntimeError(f'Math structure/token order mismatch: {ET.tostring(tree, encoding="unicode")}')
        if structure == 'mtable' and (len(tree.findall('.//mtr')) != 2 or len(tree.findall('.//mtd')) != 4):
            raise RuntimeError('Matrix row/cell relationships were lost')
        equations[source] = ET.tostring(tree, encoding='unicode')
    fallback = [node for node in nodes if node['role'] == 'math' and node['name'] == '\\notARealCommand{x}\n']
    if len(fallback) != 1 or fallback[0].get('attributes', {}).get('tag'):
        raise RuntimeError('Invalid formula must retain source without fabricated MathML')
    return dict(equations=equations, exact_source_labels=True, source_order_tokens=True,
                invalid_formula_source_fallback=True, passes=True)
