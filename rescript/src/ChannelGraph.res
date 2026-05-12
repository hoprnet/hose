// Channel graph page: manual fetch of Blokli channels with peer-id filters.

type channelData = {
  id: string,
  source: string,
  destination: string,
  status: string,
  balance: string,
  channel_epoch: int,
  ticket_index: int,
  closure_time: Js.Nullable.t<string>,
}

@scope("JSON") @val
external parseChannels: string => array<channelData> = "parse"

type response
@val external fetch: string => Js.Promise.t<response> = "fetch"
@send external text: response => Js.Promise.t<string> = "text"
@get external ok: response => bool = "ok"
@get external status: response => int = "status"

@val external encodeURIComponent: string => string = "encodeURIComponent"
@get external getValue: DomUtil.element => string = "value"
@set external setDisabled: (DomUtil.element, bool) => unit = "disabled"

let setStatus = (statusEl, message, className) => {
  DomUtil.setTextContent(statusEl, message)
  DomUtil.setClassName(statusEl, className)
}

let setEmptyVisible = (emptyEl, visible) => {
  DomUtil.setDisplay(DomUtil.getStyle(emptyEl), if visible {"block"} else {"none"})
}

let splitPeerIds = (raw: string): array<string> => {
  let normalized = Js.String2.replaceByRe(raw, %re("/[\\n\\r\\t ]+/g"), ",")
  let tokens = Js.String2.splitByRe(normalized, %re("/,+/"))
  let seen = Js.Dict.empty()

  Belt.Array.keepMap(tokens, token =>
    switch token {
    | Some(value) =>
      let trimmed = Js.String2.trim(value)
      if trimmed === "" {
        None
      } else {
        switch Js.Dict.get(seen, trimmed) {
        | Some(_) => None
        | None =>
          Js.Dict.set(seen, trimmed, true)
          Some(trimmed)
        }
      }
    | None => None
    }
  )
}

let channelRowHtml = (channel: channelData): string => {
  let closureTime = switch Js.Nullable.toOption(channel.closure_time) {
  | Some(value) => value
  | None => "\u2014"
  }

  "<td><code>" ++
  DomUtil.escapeHtml(channel.id) ++
  "</code></td>" ++
  "<td><code>" ++
  DomUtil.escapeHtml(channel.source) ++
  "</code></td>" ++
  "<td><code>" ++
  DomUtil.escapeHtml(channel.destination) ++
  "</code></td>" ++
  "<td><span class=\"badge badge-completed\">" ++
  DomUtil.escapeHtml(channel.status) ++
  "</span></td>" ++
  "<td>" ++
  DomUtil.escapeHtml(channel.balance) ++
  "</td>" ++
  "<td>" ++
  DomUtil.escapeHtml(Js.Int.toString(channel.channel_epoch)) ++
  "</td>" ++
  "<td>" ++
  DomUtil.escapeHtml(Js.Int.toString(channel.ticket_index)) ++
  "</td>" ++
  "<td>" ++ DomUtil.escapeHtml(closureTime) ++ "</td>"
}

let renderChannels = (tbodyEl, emptyEl, channels: array<channelData>) => {
  DomUtil.setInnerHTML(tbodyEl, "")

  if Js.Array2.length(channels) === 0 {
    setEmptyVisible(emptyEl, true)
  } else {
    setEmptyVisible(emptyEl, false)
    Js.Array2.forEach(channels, channel => {
      let row = DomUtil.createElement(DomUtil.doc, "tr")
      DomUtil.setInnerHTML(row, channelRowHtml(channel))
      DomUtil.appendChild(tbodyEl, row)
    })
  }
}

let buildUrl = (peerIds: array<string>, mode: string): string => {
  let modeParam = "filter_mode=" ++ encodeURIComponent(mode)
  let peerParams = peerIds->Js.Array2.map(peer => "peer_ids=" ++ encodeURIComponent(peer))->Js.Array2.joinWith("&")

  if peerParams === "" {
    "/api/channels?" ++ modeParam
  } else {
    "/api/channels?" ++ modeParam ++ "&" ++ peerParams
  }
}

let loadChannels = (~tbodyEl, ~statusEl, ~emptyEl, ~inputEl, ~modeEl, ~buttonEl) => {
  let peerIds = splitPeerIds(getValue(inputEl))
  let mode = getValue(modeEl)
  let url = buildUrl(peerIds, mode)
  let modeLabel = if mode === "both" {"both endpoints"} else {"any endpoint"}

  setDisabled(buttonEl, true)
  setStatus(statusEl, "Loading channels...", "sse-status")

  fetch(url)
  ->Js.Promise.then_(response => {
    text(response)->Js.Promise.then_(body => {
      if ok(response) {
        let channels = parseChannels(body)
        renderChannels(tbodyEl, emptyEl, channels)
        setStatus(
          statusEl,
          "Loaded " ++ Js.Int.toString(Js.Array2.length(channels)) ++ " channels (" ++ modeLabel ++ ").",
          "sse-status connected",
        )
      } else {
        setStatus(
          statusEl,
          "Request failed (" ++ Js.Int.toString(status(response)) ++ "): " ++ body,
          "sse-status disconnected",
        )
      }
      setDisabled(buttonEl, false)
      Js.Promise.resolve()
    }, _)
  }, _)
  ->Js.Promise.catch(_err => {
    setStatus(statusEl, "Network error while loading channels.", "sse-status disconnected")
    setDisabled(buttonEl, false)
    Js.Promise.resolve()
  }, _)
  ->ignore
}

let () = {
  let tbody = DomUtil.getElementById(DomUtil.doc, "channel-graph-body")
  let status = DomUtil.getElementById(DomUtil.doc, "channel-graph-status")
  let empty = DomUtil.getElementById(DomUtil.doc, "channel-graph-empty")
  let input = DomUtil.getElementById(DomUtil.doc, "peer-ids-input")
  let mode = DomUtil.getElementById(DomUtil.doc, "filter-mode-select")
  let button = DomUtil.getElementById(DomUtil.doc, "refresh-channels-btn")

  switch (
    Js.Nullable.toOption(tbody),
    Js.Nullable.toOption(status),
    Js.Nullable.toOption(empty),
    Js.Nullable.toOption(input),
    Js.Nullable.toOption(mode),
    Js.Nullable.toOption(button),
  ) {
  | (Some(tbodyEl), Some(statusEl), Some(emptyEl), Some(inputEl), Some(modeEl), Some(buttonEl)) =>
    DomUtil.addClickListener(buttonEl, _event => {
      loadChannels(~tbodyEl, ~statusEl, ~emptyEl, ~inputEl, ~modeEl, ~buttonEl)
    })
    loadChannels(~tbodyEl, ~statusEl, ~emptyEl, ~inputEl, ~modeEl, ~buttonEl)
  | _ => ()
  }
}
