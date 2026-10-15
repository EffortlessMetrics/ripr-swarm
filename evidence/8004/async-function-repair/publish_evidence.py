import pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);branch='refs/heads/rook/8004-async-function-repair-evidence'
def git(*args,data=None):return subprocess.check_output(['git',*args],input=data).decode().strip()
def tree(path):
    records=[]
    for p in sorted(path.iterdir()):
        if p.is_dir(): mode,kind,oid='040000','tree',tree(p)
        else:mode,kind,oid='100644','blob',git('hash-object','-w',str(p))
        records.append(f'{mode} {kind} {oid}\t{p.name}\n')
    return git('mktree',data=''.join(records).encode())
oid=tree(root)
for name in ['async-function-repair','8004','evidence']:
    oid=git('mktree',data=f'040000 tree {oid}\t{name}\n'.encode())
try:parent=git('rev-parse','--verify',branch)
except subprocess.CalledProcessError:parent=None
args=['commit-tree',oid]
if parent:args+=['-p',parent]
commit=git(*args,data=(sys.argv[2]+'\n').encode())
git('update-ref',branch,commit,parent or '0'*40)
print(commit)
