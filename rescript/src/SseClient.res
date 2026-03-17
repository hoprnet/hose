// SSE client wrapper around EventSource.

type eventSource

@new external makeEventSource: string => eventSource = "EventSource"

@send
external addEventListener: (eventSource, string, 'event => unit) => unit = "addEventListener"

@get external getData: 'event => string = "data"

type handler = {eventType: string, callback: string => unit}

let connect = (~url="/api/events", ~onOpen, ~onError, ~onEvent: array<handler>) => {
  let source = makeEventSource(url)

  addEventListener(source, "open", _event => {
    onOpen()
  })

  addEventListener(source, "error", _event => {
    onError()
  })

  Js.Array2.forEach(onEvent, ({eventType, callback}) => {
    addEventListener(source, eventType, event => {
      let data = getData(event)
      callback(data)
    })
  })

  source
}
