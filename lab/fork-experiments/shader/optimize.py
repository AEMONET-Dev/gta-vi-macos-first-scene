"""Test upstream dead-code and initialization switches, with identical input and precise math."""
import run
from repeat import measure

def main():
    args={'zero-init':([],['--force-zero-initialized-variables']),
          'dce':(['--dead-code-eliminate'],[]),
          'legacy':(['--legacy-structurizer'],[])}
    shader,_=run.SAMPLES['c7a']
    for label,(dxil,cross) in args.items():
        spv=run.HERE/(shader+'-'+label+'.spv')
        msl=spv.with_suffix('.metal')
        run.run([run.DXIL,run.ORIGINALS/(shader+'.dxil'),'--bindless','--validate',*dxil,'--output',spv])
        run.run([run.VALIDATOR,'--target-env','vulkan1.3',spv])
        run.run([run.CROSS,spv,*run.CROSS_FLAGS,*cross,'--output',msl])
        measure(label,label,True,loops=12)
if __name__=='__main__':
    main()
