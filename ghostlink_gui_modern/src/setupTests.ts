import '@testing-library/jest-dom'

if (typeof Element !== 'undefined') {
  Element.prototype.scrollIntoView = function () {}
}
