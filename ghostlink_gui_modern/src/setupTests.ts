import '@testing-library/jest-dom'

if (typeof Element !== 'undefined') {
  Element.prototype.scrollIntoView = function () {}
}

if (typeof document !== 'undefined' && !document.queryCommandSupported) {
  document.queryCommandSupported = () => false;
}
