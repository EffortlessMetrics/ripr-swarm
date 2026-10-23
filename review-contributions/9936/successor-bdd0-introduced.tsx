export {};
declare global { namespace JSX { type Element = any; interface IntrinsicElements { div: any; } } }
declare let test: any;
const node = <div>if (n) /a {test = null}</div>;
