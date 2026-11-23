function stringify(obj, options) {
  if (typeof options !== 'undefined') {
    return JSON.stringify(obj);
  }
  return JSON.stringify(obj, null, 2);
}
module.exports = { stringify };
