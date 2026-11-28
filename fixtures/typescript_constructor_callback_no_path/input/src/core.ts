export function constructor(
  name: string,
  init: (inst: any, def: any) => void,
): (def?: any) => any {
  return (def?: any) => {
    const inst: any = { name };
    init(inst, def);
    return inst;
  };
}
