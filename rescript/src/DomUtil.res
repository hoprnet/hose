// DOM helper utilities.

type element
type document
type style

@val external doc: document = "document"

@send
external getElementById: (document, string) => Js.Nullable.t<element> = "getElementById"

@send
external createElement: (document, string) => element = "createElement"

@send
external createTextNode: (document, string) => element = "createTextNode"

@set external setTextContent: (element, string) => unit = "textContent"

@set external setInnerHTML: (element, string) => unit = "innerHTML"

@get external getInnerHTML: element => string = "innerHTML"

@set external setClassName: (element, string) => unit = "className"

@get external getClassName: element => string = "className"

@get external getFirstChild: element => Js.Nullable.t<element> = "firstChild"

@get external getChildren: element => array<element> = "children"

@get external getChildrenLength: element => int = "childElementCount"

@send external appendChild: (element, element) => unit = "appendChild"

@send
external insertBefore: (element, element, Js.Nullable.t<element>) => unit = "insertBefore"

@send external removeChild: (element, element) => unit = "removeChild"

@get external getLastChild: element => Js.Nullable.t<element> = "lastChild"

@get external getStyle: element => style = "style"

@set external setDisplay: (style, string) => unit = "display"

let hideElement = (el: element): unit => {
  setDisplay(getStyle(el), "none")
}

@send
external addClickListener: (element, @as("click") _, 'event => unit) => unit = "addEventListener"

// Escape HTML by creating a text node and reading its parent's innerHTML.
let escapeHtml = (text: string): string => {
  let div = createElement(doc, "div")
  let textNode = createTextNode(doc, text)
  appendChild(div, textNode)
  getInnerHTML(div)
}
