<!-- section: Changed -->
- TypeScript and JavaScript: a test now relates to changed code it reaches
  through three more import shapes. A barrel that imports a name and then
  exports it (`import { f } from './x'; export { f }`, `export const g = f`)
  resolves to the declaring file like `export { f } from './x'` does
  (#15922). A namespace import of a barrel (`import * as lib from '../src';
  lib.f()`, `new lib.Cart()`) resolves each called member through the
  barrel's re-exports (#15923). A changed instance method or getter is related
  to a call on a subclass instance when the subclass extends the changed class
  by a plain identifier and does not supply the member itself, and to a call
  on a value built by an exported function factory or a static factory that
  returns `new` of the class (#15924). All three refuse rather than guess: a
  rebound namespace or import, an ambiguous or unresolved hop, a class with a
  decorator, computed key, static block or prototype write, an override, a
  spied or replaced member, or a mocked module leaves the finding as it was.
  TypeScript fact caches are rebuilt once (fact schema 11).
