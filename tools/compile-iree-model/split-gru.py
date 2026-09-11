"""Exact bidirectional GRU decomposition into forward GRUs (no weight changes)."""
import argparse
from pathlib import Path
import json
import numpy as np
import onnx
from onnx import helper as h, numpy_helper as nh

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
m = onnx.shape_inference.infer_shapes(onnx.load(args.input))
shapes = {v.name: [d.dim_value for d in v.type.tensor_type.shape.dim]
          for v in [*m.graph.input, *m.graph.value_info, *m.graph.output]}
nodes = []
count = 0
for node in m.graph.node:
    if node.op_type != 'GRU':
        nodes.append(node)
        continue
    attrs = {a.name: h.get_attribute_value(a) for a in node.attribute}
    assert attrs == {'direction': b'bidirectional', 'hidden_size': 64, 'linear_before_reset': 1}, attrs
    assert not node.input[4], 'variable sequence lengths need separate handling'
    prefix = f'iree_split_gru_{count}'
    count += 1
    seq = shapes[node.input[0]][0]
    assert seq in [40, 48]
    def const(name, value):
        name = prefix + name
        m.graph.initializer.append(nh.from_array(np.asarray(value, dtype=np.int64), name))
        return name
    reverse = const('_reverse', np.arange(seq - 1, -1, -1))
    ys, hs = [], []
    for direction in [0, 1]:
        d = f'{prefix}_d{direction}'
        idx = const(f'_index_{direction}', [direction])
        inputs = list(node.input)
        if direction:
            nodes.append(h.make_node('Gather', [inputs[0], reverse], [d+'_x'], axis=0))
            inputs[0] = d+'_x'
        for i in [1, 2, 3, 5]:
            if inputs[i]:
                name = f'{d}_input{i}'
                nodes.append(h.make_node('Gather', [inputs[i], idx], [name], axis=0))
                inputs[i] = name
        y, state = d+'_y', d+'_h'
        nodes.append(h.make_node('GRU', inputs, [y, state], name=d,
                                 direction='forward', hidden_size=64, linear_before_reset=1))
        if direction:
            nodes.append(h.make_node('Gather', [y, reverse], [d+'_y_reversed'], axis=0))
            y = d+'_y_reversed'
        ys.append(y)
        hs.append(state)
    nodes.append(h.make_node('Concat', ys, [node.output[0]], axis=1))
    if len(node.output) > 1 and node.output[1]:
        nodes.append(h.make_node('Concat', hs, [node.output[1]], axis=0))
del m.graph.node[:]
m.graph.node.extend(nodes)
m = onnx.shape_inference.infer_shapes(m)
onnx.checker.check_model(m)
assert count == 16, count
onnx.save(m, args.output)
print(json.dumps({'bidirectional_grus_replaced': count, 'forward_grus': count*2}))
