"""Compare equivalent uniform group-index reads for the two captured inputs."""
from pathlib import Path
import subprocess
import run
from repeat import measure

shader,_=run.SAMPLES['c7a']
source=(run.HERE/(shader+'-safe.metal')).read_text()
old='spvSubgroupBroadcastFirst(as_type<uint>(_17[_133].read(uint(gl_WorkGroupID.x)).x))'
assert source.count(old)==1
variants={
    'uniform-read':'as_type<uint>(_17[_133].read(uint(gl_WorkGroupID.x)).x)',
    'uniform-uint':'_28[_133].read(uint(gl_WorkGroupID.x)).x',
}
for label,read in variants.items():
    (run.HERE/(shader+'-'+label+'.metal')).write_text(source.replace(old,read))
    measure(label,label,True,loops=12)
