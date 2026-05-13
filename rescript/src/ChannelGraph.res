// Channel graph page: manual fetch, composite filter tokens, sortable table, endpoint identity view switch.

type channelData = {
  id: string,
  source: string,
  destination: string,
  source_key_id: string,
  destination_key_id: string,
  source_chain_key: Js.Nullable.t<string>,
  destination_chain_key: Js.Nullable.t<string>,
  source_packet_key: Js.Nullable.t<string>,
  destination_packet_key: Js.Nullable.t<string>,
  source_peer_id: Js.Nullable.t<string>,
  destination_peer_id: Js.Nullable.t<string>,
  status: string,
  balance: string,
  channel_epoch: int,
  ticket_index: int,
  closure_time: Js.Nullable.t<string>,
}

type sortDirection = Unsorted | Asc | Desc

@scope("JSON") @val
external parseChannels: string => array<channelData> = "parse"

type response
type headers
@val external fetch: string => Js.Promise.t<response> = "fetch"
@send external text: response => Js.Promise.t<string> = "text"
@get external ok: response => bool = "ok"
@get external status: response => int = "status"
@get external getHeaders: response => headers = "headers"
@send external getHeader: (headers, string) => Js.Nullable.t<string> = "get"

@val external encodeURIComponent: string => string = "encodeURIComponent"
@get external getValue: DomUtil.element => string = "value"
@set external setDisabled: (DomUtil.element, bool) => unit = "disabled"
@send external getAttribute: (DomUtil.element, string) => Js.Nullable.t<string> = "getAttribute"
@send external addChangeListener: (DomUtil.element, @as("change") _, 'event => unit) => unit = "addEventListener"

let setStatus = (statusEl, message, className) => {
  DomUtil.setTextContent(statusEl, message)
  DomUtil.setClassName(statusEl, className)
}

let setEmptyVisible = (emptyEl, visible) => {
  DomUtil.setDisplay(DomUtil.getStyle(emptyEl), if visible {"block"} else {"none"})
}

let splitFilterTerms = (raw: string): array<string> => {
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

let endpointValue = (
  ~keyId: string,
  ~chainKey: Js.Nullable.t<string>,
  ~packetKey: Js.Nullable.t<string>,
  ~peerId: Js.Nullable.t<string>,
  ~mode: string,
) =>
  switch mode {
  | "chain_key" => Js.Nullable.toOption(chainKey)->Belt.Option.getWithDefault("-")
  | "packet_key" =>
    switch Js.Nullable.toOption(packetKey) {
    | Some(value) => "0x" ++ value
    | None => "-"
    }
  | "peer_id" => Js.Nullable.toOption(peerId)->Belt.Option.getWithDefault("-")
  | _ => keyId
  }

let channelRowHtml = (channel: channelData, endpointMode: string): string => {
  let closureTime = switch Js.Nullable.toOption(channel.closure_time) {
  | Some(value) => value
  | None => "-"
  }
  let sourceDisplay = endpointValue(
    ~keyId=channel.source_key_id,
    ~chainKey=channel.source_chain_key,
    ~packetKey=channel.source_packet_key,
    ~peerId=channel.source_peer_id,
    ~mode=endpointMode,
  )
  let destinationDisplay = endpointValue(
    ~keyId=channel.destination_key_id,
    ~chainKey=channel.destination_chain_key,
    ~packetKey=channel.destination_packet_key,
    ~peerId=channel.destination_peer_id,
    ~mode=endpointMode,
  )

  "<td><code>" ++
  DomUtil.escapeHtml(channel.id) ++
  "</code></td>" ++
  "<td><code>" ++
  DomUtil.escapeHtml(sourceDisplay) ++
  "</code></td>" ++
  "<td><code>" ++
  DomUtil.escapeHtml(destinationDisplay) ++
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

let compareString = (a, b) => {
  if a < b {
    -1
  } else if a > b {
    1
  } else {
    0
  }
}

let compareIntLocal = (a: int, b: int) =>
  if a < b {
    -1
  } else if a > b {
    1
  } else {
    0
  }

let compareClosureTimeNullLast = (a: Js.Nullable.t<string>, b: Js.Nullable.t<string>) => {
  switch (Js.Nullable.toOption(a), Js.Nullable.toOption(b)) {
  | (None, None) => 0
  | (None, Some(_)) => 1
  | (Some(_), None) => -1
  | (Some(av), Some(bv)) => compareString(av, bv)
  }
}

let compareByKey = (a: channelData, b: channelData, key: string, endpointMode: string) => {
  switch key {
  | "id" => compareString(a.id, b.id)
  | "source" =>
    compareString(
      endpointValue(
        ~keyId=a.source_key_id,
        ~chainKey=a.source_chain_key,
        ~packetKey=a.source_packet_key,
        ~peerId=a.source_peer_id,
        ~mode=endpointMode,
      ),
      endpointValue(
        ~keyId=b.source_key_id,
        ~chainKey=b.source_chain_key,
        ~packetKey=b.source_packet_key,
        ~peerId=b.source_peer_id,
        ~mode=endpointMode,
      ),
    )
  | "destination" =>
    compareString(
      endpointValue(
        ~keyId=a.destination_key_id,
        ~chainKey=a.destination_chain_key,
        ~packetKey=a.destination_packet_key,
        ~peerId=a.destination_peer_id,
        ~mode=endpointMode,
      ),
      endpointValue(
        ~keyId=b.destination_key_id,
        ~chainKey=b.destination_chain_key,
        ~packetKey=b.destination_packet_key,
        ~peerId=b.destination_peer_id,
        ~mode=endpointMode,
      ),
    )
  | "status" => compareString(a.status, b.status)
  | "balance" => compareString(a.balance, b.balance)
  | "channel_epoch" => compareIntLocal(a.channel_epoch, b.channel_epoch)
  | "ticket_index" => compareIntLocal(a.ticket_index, b.ticket_index)
  | "closure_time" => compareClosureTimeNullLast(a.closure_time, b.closure_time)
  | _ => 0
  }
}

let sortedChannels = (
  channels: array<channelData>,
  sortKey: option<string>,
  sortDirection: sortDirection,
  endpointMode: string,
) => {
  let copy = Belt.Array.map(channels, c => c)

  switch (sortKey, sortDirection) {
  | (Some(key), Asc) =>
    Js.Array2.sortInPlaceWith(copy, (a, b) => compareByKey(a, b, key, endpointMode))->ignore
    copy
  | (Some(key), Desc) =>
    Js.Array2.sortInPlaceWith(copy, (a, b) => compareByKey(b, a, key, endpointMode))->ignore
    copy
  | _ => copy
  }
}

let renderChannels = (
  tbodyEl,
  emptyEl,
  channels: array<channelData>,
  sortKey: option<string>,
  sortDirection: sortDirection,
  endpointMode: string,
) => {
  let rows = sortedChannels(channels, sortKey, sortDirection, endpointMode)
  DomUtil.setInnerHTML(tbodyEl, "")

  if Js.Array2.length(rows) === 0 {
    setEmptyVisible(emptyEl, true)
  } else {
    setEmptyVisible(emptyEl, false)
    Js.Array2.forEach(rows, channel => {
      let row = DomUtil.createElement(DomUtil.doc, "tr")
      DomUtil.setInnerHTML(row, channelRowHtml(channel, endpointMode))
      DomUtil.appendChild(tbodyEl, row)
    })
  }
}

let buildUrl = (terms: array<string>, mode: string): string => {
  let modeParam = "filter_mode=" ++ encodeURIComponent(mode)
  let termParams = terms->Js.Array2.map(term => "peer_ids=" ++ encodeURIComponent(term))->Js.Array2.joinWith("&")

  if termParams === "" {
    "/api/channels?" ++ modeParam
  } else {
    "/api/channels?" ++ modeParam ++ "&" ++ termParams
  }
}

let updateSortButtons = (sortButtons: array<DomUtil.element>, activeKey: option<string>, direction: sortDirection) => {
  Js.Array2.forEach(sortButtons, button => {
    let key = getAttribute(button, "data-sort-key")->Js.Nullable.toOption
    switch key {
    | Some(k) =>
      let className = switch (activeKey, direction) {
      | (Some(active), Asc) if active === k => "sort-btn sort-asc"
      | (Some(active), Desc) if active === k => "sort-btn sort-desc"
      | _ => "sort-btn"
      }
      DomUtil.setClassName(button, className)
    | None => ()
    }
  })
}

let loadChannels = (
  ~tbodyEl,
  ~statusEl,
  ~emptyEl,
  ~inputEl,
  ~modeEl,
  ~buttonEl,
  ~channelsRef,
  ~sortKeyRef,
  ~sortDirectionRef,
  ~endpointModeRef,
) => {
  let terms = splitFilterTerms(getValue(inputEl))
  let mode = getValue(modeEl)
  let url = buildUrl(terms, mode)
  let modeLabel = if mode === "both" {"both endpoints"} else {"any endpoint"}

  setDisabled(buttonEl, true)
  setStatus(statusEl, "Loading channels...", "sse-status")

  fetch(url)
  ->Js.Promise.then_(response => {
    text(response)->Js.Promise.then_(body => {
      if ok(response) {
        let channels = parseChannels(body)
        channelsRef := channels
        renderChannels(
          tbodyEl,
          emptyEl,
          channelsRef.contents,
          sortKeyRef.contents,
          sortDirectionRef.contents,
          endpointModeRef.contents,
        )

        let headers = getHeaders(response)
        let unresolvedCount =
          getHeader(headers, "x-hose-filter-unresolved-count")
          ->Js.Nullable.toOption
          ->Belt.Option.getWithDefault("0")
        let unresolvedTerms =
          getHeader(headers, "x-hose-filter-unresolved")->Js.Nullable.toOption->Belt.Option.getWithDefault("")

        let base = "Loaded " ++ Js.Int.toString(Js.Array2.length(channels)) ++ " channels (" ++ modeLabel ++ ")."
        if unresolvedCount !== "0" {
          let detail = if unresolvedTerms === "" {"unresolved filter terms"} else {unresolvedTerms}
          setStatus(
            statusEl,
            base ++ " Ignored " ++ unresolvedCount ++ " unresolved: " ++ detail,
            "sse-status disconnected",
          )
        } else {
          setStatus(statusEl, base, "sse-status connected")
        }
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
  let endpointMode = DomUtil.getElementById(DomUtil.doc, "endpoint-display-select")
  let refreshBtn = DomUtil.getElementById(DomUtil.doc, "refresh-channels-btn")

  let sortId = DomUtil.getElementById(DomUtil.doc, "sort-id")
  let sortSource = DomUtil.getElementById(DomUtil.doc, "sort-source")
  let sortDestination = DomUtil.getElementById(DomUtil.doc, "sort-destination")
  let sortStatus = DomUtil.getElementById(DomUtil.doc, "sort-status")
  let sortBalance = DomUtil.getElementById(DomUtil.doc, "sort-balance")
  let sortEpoch = DomUtil.getElementById(DomUtil.doc, "sort-epoch")
  let sortTicket = DomUtil.getElementById(DomUtil.doc, "sort-ticket-index")
  let sortClosure = DomUtil.getElementById(DomUtil.doc, "sort-closure-time")

  switch (
    Js.Nullable.toOption(tbody),
    Js.Nullable.toOption(status),
    Js.Nullable.toOption(empty),
    Js.Nullable.toOption(input),
    Js.Nullable.toOption(mode),
    Js.Nullable.toOption(endpointMode),
    Js.Nullable.toOption(refreshBtn),
    Js.Nullable.toOption(sortId),
    Js.Nullable.toOption(sortSource),
    Js.Nullable.toOption(sortDestination),
    Js.Nullable.toOption(sortStatus),
    Js.Nullable.toOption(sortBalance),
    Js.Nullable.toOption(sortEpoch),
    Js.Nullable.toOption(sortTicket),
    Js.Nullable.toOption(sortClosure),
  ) {
  | (
      Some(tbodyEl),
      Some(statusEl),
      Some(emptyEl),
      Some(inputEl),
      Some(modeEl),
      Some(endpointModeEl),
      Some(refreshBtnEl),
      Some(sortIdEl),
      Some(sortSourceEl),
      Some(sortDestinationEl),
      Some(sortStatusEl),
      Some(sortBalanceEl),
      Some(sortEpochEl),
      Some(sortTicketEl),
      Some(sortClosureEl),
    ) =>
    let channelsRef: ref<array<channelData>> = ref([])
    let sortKeyRef: ref<option<string>> = ref(None)
    let sortDirectionRef = ref(Unsorted)
    let endpointModeRef = ref(getValue(endpointModeEl))
    let sortButtons = [sortIdEl, sortSourceEl, sortDestinationEl, sortStatusEl, sortBalanceEl, sortEpochEl, sortTicketEl, sortClosureEl]

    let rerender = () =>
      renderChannels(
        tbodyEl,
        emptyEl,
        channelsRef.contents,
        sortKeyRef.contents,
        sortDirectionRef.contents,
        endpointModeRef.contents,
      )

    let toggleSort = (key: string) => {
      switch (sortKeyRef.contents, sortDirectionRef.contents) {
      | (Some(active), Unsorted) if active === key => sortDirectionRef := Asc
      | (Some(active), Asc) if active === key => sortDirectionRef := Desc
      | (Some(active), Desc) if active === key =>
        sortKeyRef := None
        sortDirectionRef := Unsorted
      | _ =>
        sortKeyRef := Some(key)
        sortDirectionRef := Asc
      }
      updateSortButtons(sortButtons, sortKeyRef.contents, sortDirectionRef.contents)
      rerender()
    }

    Js.Array2.forEach(sortButtons, button => {
      DomUtil.addClickListener(button, _event => {
        switch getAttribute(button, "data-sort-key")->Js.Nullable.toOption {
        | Some(key) => toggleSort(key)
        | None => ()
        }
      })
    })

    addChangeListener(endpointModeEl, _event => {
      endpointModeRef := getValue(endpointModeEl)
      rerender()
    })

    DomUtil.addClickListener(refreshBtnEl, _event => {
      loadChannels(
        ~tbodyEl,
        ~statusEl,
        ~emptyEl,
        ~inputEl,
        ~modeEl,
        ~buttonEl=refreshBtnEl,
        ~channelsRef,
        ~sortKeyRef,
        ~sortDirectionRef,
        ~endpointModeRef,
      )
    })

    loadChannels(
      ~tbodyEl,
      ~statusEl,
      ~emptyEl,
      ~inputEl,
      ~modeEl,
      ~buttonEl=refreshBtnEl,
      ~channelsRef,
      ~sortKeyRef,
      ~sortDirectionRef,
      ~endpointModeRef,
    )
  | _ => ()
  }
}
