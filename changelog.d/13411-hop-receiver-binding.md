<!-- section: Internal / CI -->

- Helper method hops now carry how their receiver is bound (`HopReceiver` with a `ReceiverBinding`), and the consumers that read it as a caller parameter go through one accessor; no analysis behavior changes (#13411, 2/3).
