// Trace Inspector page: SSE → table rows, 100-row cap, connection status.

let maxRows = 100

type traceData = {
  timestamp: string,
  peer_id: string,
  span_name: string,
  trace_id: string,
  routing_decision: string,
  attributes: Js.Json.t,
}

@scope("JSON") @val
external parseTrace: string => traceData = "parse"

// Date bindings at module level
type date
@new external makeDate: string => date = "Date"
@send external toLocaleTimeString: date => string = "toLocaleTimeString"

let renderTrace = (tbodyEl, emptyEl, trace) => {
  DomUtil.hideElement(emptyEl)

  let row = DomUtil.createElement(DomUtil.doc, "tr")

  // Format timestamp
  let ts = makeDate(trace.timestamp)
  let timeStr = toLocaleTimeString(ts)

  // Truncate trace ID
  let shortTrace = if Js.String2.length(trace.trace_id) > 16 {
    Js.String2.slice(trace.trace_id, ~from=0, ~to_=16) ++ "..."
  } else {
    trace.trace_id
  }

  // Badge class
  let badgeClass = if trace.routing_decision === "retain" {
    "badge badge-active"
  } else {
    "badge badge-completed"
  }

  // Format attributes
  let attrStr = switch Js.Json.classify(trace.attributes) {
  | Js.Json.JSONObject(dict) =>
    Js.Dict.entries(dict)
    ->Js.Array2.map(((k, v)) => {
      let valStr = switch Js.Json.classify(v) {
      | Js.Json.JSONString(s) => s
      | _ => Js.Json.stringify(v)
      }
      k ++ "=" ++ valStr
    })
    ->Js.Array2.joinWith(", ")
  | _ => ""
  }

  let attrDisplay = if attrStr === "" {
    "\u2014"
  } else {
    attrStr
  }

  DomUtil.setInnerHTML(
    row,
    "<td>" ++
    DomUtil.escapeHtml(timeStr) ++
    "</td>" ++
    "<td><span class=\"peer-tag\">" ++
    DomUtil.escapeHtml(trace.peer_id) ++
    "</span></td>" ++
    "<td>" ++
    DomUtil.escapeHtml(trace.span_name) ++
    "</td>" ++
    "<td><code>" ++
    DomUtil.escapeHtml(shortTrace) ++
    "</code></td>" ++
    "<td><span class=\"" ++
    badgeClass ++
    "\">" ++
    DomUtil.escapeHtml(trace.routing_decision) ++
    "</span></td>" ++
    "<td style=\"font-size:12px;color:#666\">" ++
    DomUtil.escapeHtml(attrDisplay) ++ "</td>",
  )

  DomUtil.insertBefore(tbodyEl, row, DomUtil.getFirstChild(tbodyEl))

  // Cap at maxRows
  let rec trimRows = () => {
    if DomUtil.getChildrenLength(tbodyEl) > maxRows {
      switch Js.Nullable.toOption(DomUtil.getLastChild(tbodyEl)) {
      | Some(last) =>
        DomUtil.removeChild(tbodyEl, last)
        trimRows()
      | None => ()
      }
    }
  }
  trimRows()
}

let () = {
  let tbody = DomUtil.getElementById(DomUtil.doc, "trace-body")
  let status = DomUtil.getElementById(DomUtil.doc, "status")
  let empty = DomUtil.getElementById(DomUtil.doc, "empty-state")
  let pauseBtn = DomUtil.getElementById(DomUtil.doc, "pause-btn")

  switch (
    Js.Nullable.toOption(tbody),
    Js.Nullable.toOption(status),
    Js.Nullable.toOption(empty),
    Js.Nullable.toOption(pauseBtn),
  ) {
  | (Some(tbodyEl), Some(statusEl), Some(emptyEl), Some(pauseBtnEl)) =>
    let paused = ref(false)
    let buffer = ref([])

    // Wire pause/resume button
    DomUtil.addClickListener(pauseBtnEl, _event => {
      paused := !paused.contents
      if paused.contents {
        DomUtil.setTextContent(pauseBtnEl, "Resume")
        DomUtil.setClassName(pauseBtnEl, "badge badge-completed")
      } else {
        // Flush buffered traces
        Js.Array2.forEach(buffer.contents, trace => renderTrace(tbodyEl, emptyEl, trace))
        buffer := []
        DomUtil.setTextContent(pauseBtnEl, "Pause")
        DomUtil.setClassName(pauseBtnEl, "badge badge-active")
      }
    })

    let _source = SseClient.connect(
      ~onOpen=() => {
        DomUtil.setTextContent(statusEl, "Connected \u2014 listening for traces")
        DomUtil.setClassName(statusEl, "sse-status connected")
      },
      ~onError=() => {
        DomUtil.setTextContent(statusEl, "Disconnected \u2014 reconnecting...")
        DomUtil.setClassName(statusEl, "sse-status disconnected")
      },
      ~onEvent=[
        {
          eventType: "trace_sampled",
          callback: data => {
            let trace = parseTrace(data)
            if paused.contents {
              buffer := Js.Array2.concat(buffer.contents, [trace])
            } else {
              renderTrace(tbodyEl, emptyEl, trace)
            }
          },
        },
      ],
    )
  | _ => ()
  }
}
