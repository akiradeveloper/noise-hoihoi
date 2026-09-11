"""Replace this model's GRUs with Vulkan dispatches; same FP32 weights/equations."""
from pathlib import Path
import re
import argparse

p=Path(__file__).resolve().parent
parser=argparse.ArgumentParser()
parser.add_argument('--input',type=Path,default=p/'model.mlir')
parser.add_argument('--output',type=Path,default=p/'custom-gru.mlir')
parser.add_argument('--packed-projection',action='store_true')
parser.add_argument('--packed-recurrence',action='store_true')
args=parser.parse_args()
source=args.input.read_text()
target=(p/'vulkan-target.mlirattr').read_text().strip()
header=f'#gru_target = {target}\n'
executables=[]
executables.append('''
  hal.executable.source private @temporal_conv attributes {
    objects = #hal.executable.objects<{#gru_target = [#hal.executable.object<{path = "temporal-conv.spv"}>]}>
  } {
    hal.executable.export public @main ordinal(0)
      layout(#hal.pipeline.layout<bindings = [
        #hal.pipeline.binding<storage_buffer, ReadOnly>,
        #hal.pipeline.binding<storage_buffer, ReadOnly>,
        #hal.pipeline.binding<storage_buffer>]>)
      count(%device: !hal.device) -> (index,index,index) {
        %x = arith.constant 8 : index
        %one = arith.constant 1 : index
        hal.return %x, %one, %one : index,index,index
      }
  }
''')
executables.append('''
  hal.executable.source private @grouped_matmul attributes {
    objects = #hal.executable.objects<{#gru_target = [#hal.executable.object<{path = "grouped-matmul.spv"}>]}>
  } {
    hal.executable.export public @main ordinal(0)
      layout(#hal.pipeline.layout<constants = 3, bindings = [
        #hal.pipeline.binding<storage_buffer, ReadOnly>,
        #hal.pipeline.binding<storage_buffer, ReadOnly>,
        #hal.pipeline.binding<storage_buffer>]>)
      count(%device: !hal.device, %n: index) -> (index,index,index) {
        %x = affine.apply affine_map<()[s0] -> (s0 ceildiv 64)>()[%n]
        %one = arith.constant 1 : index
        hal.return %x, %one, %one : index,index,index
      }
  }
''')
for name, inputs, count in [('project',3,'affine.apply affine_map<()[s0] -> (s0 * 6)>()[%n]'),
                            ('recur',4,'arith.constant 2 : index')]:
    bindings=', '.join(['#hal.pipeline.binding<storage_buffer, ReadOnly>']*inputs+['#hal.pipeline.binding<storage_buffer>'])
    executables.append(f'''
  hal.executable.source private @gru_{name} attributes {{
    objects = #hal.executable.objects<{{#gru_target = [#hal.executable.object<{{path = "gru-{name}.spv"}}>]}}>
  }} {{
    hal.executable.export public @main ordinal(0)
      layout(#hal.pipeline.layout<constants = 1, bindings = [{bindings}]>)
      count(%device: !hal.device, %n: index) -> (index,index,index) {{
        %x = {count}
        %one = arith.constant 1 : index
        hal.return %x, %one, %one : index,index,index
      }}
  }}
''')
lines=[]; n=0; grouped=0; convs=0
for line in source.splitlines():
    if 'torch.operator "onnx.Conv"' in line and '!torch.vtensor<[5,32,5,1],f32>' in line:
        types=re.findall(r'!torch.vtensor<\[([^]]*)\],f32>',line)
        assert types==['1,32,5,96','5,32,5,1','1,5,1,96'],types
        out,lhs,rhs=re.search(r'(%\d+) = torch.operator "onnx.Conv"\((%\d+), (%\d+)\)',line).groups()
        b=f'%conv{convs}';convs+=1
        ts=['tensor<'+s.replace(',','x')+'xf32>' for s in types]
        lines.extend([
            f'    {b}_a = torch_c.to_builtin_tensor {lhs} : !torch.vtensor<[{types[0]}],f32> -> {ts[0]}',
            f'    {b}_b = torch_c.to_builtin_tensor {rhs} : !torch.vtensor<[{types[1]}],f32> -> {ts[1]}',
            f'    {b}_y = flow.dispatch @temporal_conv::@main[]({b}_a, {b}_b) : ({ts[0]}, {ts[1]}) -> {ts[2]}',
            f'    {out} = torch_c.from_builtin_tensor {b}_y : {ts[2]} -> !torch.vtensor<[{types[2]}],f32>'
        ])
        continue
    if 'torch.operator "onnx.MatMul"' in line:
        types=re.findall(r'!torch.vtensor<\[([^]]*)\],f32>',line)
        if len(types)==3 and len(types[1].split(','))==3:
            out,lhs,rhs=re.search(r'(%\d+) = torch.operator "onnx.MatMul"\((%\d+), (%\d+)\)',line).groups()
            groups,kdim,ndim=map(int,types[1].split(','))
            assert list(map(int,types[0].split(',')))==[1,groups,1,kdim]
            assert list(map(int,types[2].split(',')))==[1,groups,1,ndim]
            b=f'%grouped{grouped}';grouped+=1
            ts=['tensor<'+s.replace(',','x')+'xf32>' for s in types]
            lines.extend([
                f'    {b}_a = torch_c.to_builtin_tensor {lhs} : !torch.vtensor<[{types[0]}],f32> -> {ts[0]}',
                f'    {b}_b = torch_c.to_builtin_tensor {rhs} : !torch.vtensor<[{types[1]}],f32> -> {ts[1]}',
                f'    {b}_k = arith.constant {kdim} : i32',
                f'    {b}_n = arith.constant {ndim} : i32',
                f'    {b}_total = arith.constant {groups*ndim} : i32',
                f'    {b}_work = arith.constant {groups*ndim} : index',
                f'    {b}_y = flow.dispatch @grouped_matmul::@main[{b}_work]({b}_k, {b}_n, {b}_total, {b}_a, {b}_b) : (i32, i32, i32, {ts[0]}, {ts[1]}) -> {ts[2]}',
                f'    {out} = torch_c.from_builtin_tensor {b}_y : {ts[2]} -> !torch.vtensor<[{types[2]}],f32>'
            ])
            continue
    if 'torch.operator "onnx.GRU"' not in line:
        lines.append(line)
        continue
    match=re.search(r'(%\d+):2 = torch.operator "onnx.GRU"\(([^)]+)\)',line)
    assert match
    out=match[1]; operands=match[2].split(', ')
    types=re.findall(r'!torch.vtensor<\[([^]]*)\],f32>',line)
    assert len(types)==7, types
    t=int(types[0].split(',')[0]); assert t in [40,48]
    assert not re.search(re.escape(out)+r'#1\b',source), 'Y_h is unexpectedly used'
    b=f'%custom{n}'; n+=1
    actual_args=[operands[i] for i in [0,1,2,3,5]]
    if args.packed_projection:
        lines.append(f'    {b}_wt = torch.operator "onnx.Transpose"({actual_args[1]}) {{torch.onnx.perm = [0 : si64, 2 : si64, 1 : si64]}} : (!torch.vtensor<[2,192,64],f32>) -> !torch.vtensor<[2,64,192],f32>')
        actual_args[1]=b+'_wt'
        types[1]='2,64,192'
    if args.packed_recurrence:
        lines.append(f'    {b}_rt = torch.operator "onnx.Transpose"({actual_args[2]}) {{torch.onnx.perm = [0 : si64, 2 : si64, 1 : si64]}} : (!torch.vtensor<[2,192,64],f32>) -> !torch.vtensor<[2,64,192],f32>')
        actual_args[2]=b+'_rt'
        types[2]='2,64,192'
    builtin=[]
    for i,(arg,shape) in enumerate(zip(actual_args,types[:5])):
        ty='tensor<'+shape.replace(',','x')+'xf32>'
        builtin.append(ty)
        lines.append(f'    {b}_{i} = torch_c.to_builtin_tensor {arg} : !torch.vtensor<[{shape}],f32> -> {ty}')
    lines.extend([
      f'    {b}_n = arith.constant {t} : index',
      f'    {b}_t = arith.constant {t} : i32',
      f'    {b}_p = flow.dispatch @gru_project::@main[{b}_n]({b}_t, {b}_0, {b}_1, {b}_3) : (i32, {builtin[0]}, {builtin[1]}, {builtin[3]}) -> tensor<2x{t}x192xf32>',
      f'    {b}_y = flow.dispatch @gru_recur::@main[{b}_n]({b}_t, {b}_p, {b}_2, {b}_3, {b}_4) : (i32, tensor<2x{t}x192xf32>, {builtin[2]}, {builtin[3]}, {builtin[4]}) -> tensor<{t}x2x1x64xf32>',
      f'    {out} = torch_c.from_builtin_tensor {b}_y : tensor<{t}x2x1x64xf32> -> !torch.vtensor<[{t},2,1,64],f32>'
    ])
result='\n'.join(lines)
for line in source.splitlines():
    if 'torch.operator "onnx.GRU"' in line:
        out=re.search(r'(%\d+):2',line)[1]
        result=re.sub(re.escape(out)+r'#0\b',out,result)
result=result.replace('module {','module {\n'+''.join(executables),1)
if args.packed_projection:
    result=result.replace('path = "gru-project.spv"','path = "gru-project-packed.spv"')
if args.packed_recurrence:
    result=result.replace('path = "gru-recur.spv"','path = "gru-recur-packed.spv"')
assert (n, grouped, convs) == (16, 10, 2), (n, grouped, convs)
args.output.write_text(header+result+'\n')
print(f'injected {n} GRUs, {grouped} grouped matmuls, {convs} temporal convolutions')
