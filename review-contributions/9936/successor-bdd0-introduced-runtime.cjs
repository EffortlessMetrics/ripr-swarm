const ts = require('/workspace/9936-tools/node_modules/typescript');
const cases = [
 ['private-write','ts','class C { #if(x: number) { return x; } f() { return this.#if(8) /(+`${(test = null, 2)}`) / 2; } } new C().f();',true],
 ['private-no-write','ts','class C { #if(x: number) { return x; } f() { return this.#if(8) /(+`test = null`) / 2; } } new C().f();',false],
 ['jsx-write','tsx','const node = <div>if (n) /a {test = null}</div>;',true],
 ['jsx-no-write','tsx','const node = <div>test = null</div>;',false],
 ['assertion-write','ts','type T = number; const n = 8; const typed = <T>n; const b = n</T>`/.source.length; test = null;',true],
 ['assertion-no-write','ts','type T = number; const n = 8; const typed = <T>n; const b = n</T>test = null/.source.length;',false],
];
for (const [label,ext,tail,expected] of cases) {
 const source = 'let test: any = () => {};\n'+tail+'\nreturn test === null;';
 const output = ts.transpileModule(source,{fileName:'control.'+ext,compilerOptions:{target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.React}}).outputText;
 const actual = new Function('React',output)({createElement:(...args)=>({args})});
 if(actual!==expected) throw new Error(label+': '+actual);
 console.log(label+': expected='+expected+' actual='+actual);
}
console.log('TypeScript '+ts.version+': 6 introduced-witness native runtime controls passed');
