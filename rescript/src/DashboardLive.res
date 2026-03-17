// Dashboard live counters: SSE events trigger API fetches to update counts.

type counterConfig = {
  eventType: string,
  apiUrl: string,
  counterAttr: string,
}

let counters = [
  {eventType: "peer_seen", apiUrl: "/api/peers", counterAttr: "peers"},
  {eventType: "session_observed", apiUrl: "/api/sessions", counterAttr: "sessions"},
  {
    eventType: "debug_session_updated",
    apiUrl: "/api/debug-sessions",
    counterAttr: "debug-sessions",
  },
]

@val external fetch: string => Js.Promise.t<'response> = "fetch"
@send external json: 'response => Js.Promise.t<Js.Json.t> = "json"

@scope("document") @val
external querySelector: string => Js.Nullable.t<DomUtil.element> = "querySelector"

let updateCounter = (apiUrl: string, counterAttr: string) => {
  fetch(apiUrl)
  ->Js.Promise.then_(response => {
    json(response)
  }, _)
  ->Js.Promise.then_(data => {
    switch Js.Json.classify(data) {
    | Js.Json.JSONArray(arr) =>
      let count = Js.Array2.length(arr)
      switch Js.Nullable.toOption(querySelector("[data-counter=\"" ++ counterAttr ++ "\"]")) {
      | Some(el) => DomUtil.setTextContent(el, Js.Int.toString(count))
      | None => ()
      }
    | _ => ()
    }
    Js.Promise.resolve()
  }, _)
  ->Js.Promise.catch(_err => {
    // Silently ignore fetch errors; counters will update on next event
    Js.Promise.resolve()
  }, _)
  ->ignore
}

let () = {
  let handlers = Js.Array2.map(counters, ({eventType, apiUrl, counterAttr}): SseClient.handler => {
    eventType,
    callback: _data => {
      updateCounter(apiUrl, counterAttr)
    },
  })

  let _source = SseClient.connect(
    ~onOpen=() => (),
    ~onError=() => (),
    ~onEvent=handlers,
  )
}
