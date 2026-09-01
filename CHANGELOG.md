# Changelog

## [0.3.3] 2026-09-01

### Changed
- startup: payany checks its command arguments before it starts. It disables itself with an error message when a check fails.
- build: the optimized release profile now strips debug symbols and aborts on panic.

### Fixed
- budget: paying a bare bolt12 offer or a bip353 address failed again when a budget is set. The 0.3.2 removal of their handling caused the failure. Payany now fetches the invoice first so the budget check can account for it.
- payany-xpay-handle-pay: ``pay`` failed again for a bare offer or a bip353 address. The 0.3.2 removal of their handling caused the failure. ``pay`` now passes them to ``xpay`` unchanged. ``maxfeepercent``/``exemptfee``/``exclude`` cannot be converted for them, use ``maxfee`` instead.
- bip353 addresses: the ``message`` argument was dropped again. The 0.3.2 removal of their handling caused the failure. Payany now forwards it as ``payer_note`` when the command supports it.
- budget: concurrent ``pay``/``xpay``/``renepay`` could race each other and exceed the budget, budget checks are now serialized and payments that passed the check are accounted for until they either show up in ``listsendpays`` or 10 seconds have passed
- budget: the check no longer panics when the payment command has no first argument.
- budget: the check now finds the invoice in the ``bolt11`` argument too.
- budget: payany now checks an amountless invoice against the budget when ``amount_msat`` is given.
- lnurl: the ``message`` argument is percent-encoded before it is added to the callback URL. This change prevents special characters from changing the URL parameters.
- lnurl: callbacks that already contain query parameters are now supported, the ``amount`` and ``comment`` parameters are appended correctly instead of corrupting the URL.
- lnurl: the ``comment`` length check now counts characters, not bytes.
- lnurl: the description hash is no longer required. Payany still checks it when the invoice carries it.
- lnurl: a decoded LNURL must be an ``https`` clearnet link or an ``http`` onion link. Local test hosts (``localhost``, ``127.0.0.1``, ``[::1]``) use ``http``. Payany checks the callback URL the same way.
- bolt12 offers: the ``message`` argument is now forwarded as ``payer_note`` when the command supports it. Setting both ``message`` and ``payer_note`` returns an error. A ``message`` on a command without the ``payer_note`` argument returns an error that asks you to upgrade CLN.
- lnaddress: only exact ``localhost``/``127.0.0.1``/``[::1]`` hosts are fetched via ``http``, any other domain that merely contains them (e.g. ``mylocalhost.com``) is fetched via ``https``. ``.onion`` addresses are fetched via ``http`` so they work when CLN is configured to use tor.
- budget: ``payany-budget-per`` is now parsed strictly, values like ``garbage1week``, ``1week extra`` or ``1.5h`` are rejected instead of being silently accepted. The time period is checked for overflow and absurd values can no longer wrap around and disable the budget check.

## [0.3.2] 2026-06-09

### Removed
- starting with CLN v25.09 `xpay` can handle offers and bip353 directly so handling is removed here.

### Changed
- updated cln-rpc and cln-plugin to v0.7

### Fixed
- handle renepay and pay deprecation

## [0.3.1] 2026-04-03

### Changed
- updated cln-rpc and cln-plugin dependencies
- make use of new HookBuilder to only intercept relevant rpc commands and not all of them, reducing CPU useage

## [0.3.0] 2025-07-07
### Removed
- :warning: ``payany-dns``: option removed (you have to remove it from your config if you have set it!) in favor of trying them all one by one and using tor if cln is configured to `always-use-proxy`

### Changed
- All lookups are proxied through tor if CLN is configured with a `proxy` and `always-use-proxy` is set to `true`
- If one DNS fails the next one is tried isntead of immediately giving up

### Added
- Explicit timeout of 30s for all lookups

## [0.2.5] 2025-04-17
### Fixed
- ``payany-xpay-handle-pay``: wallets sending ``maxfeepercent`` as a string now work as expected

## [0.2.4] 2025-04-10
### Changed
- removed extra RRSIG query for BIP-353 since some DNS servers don't respond to them, instead rely on hickory's proof status alone

## [0.2.3] 2025-04-09
### Changed
- use DNS over HTTPS with included root certificates, so you won't get censored by your router or ISP

## [0.2.2] 2025-04-08

### Fixed
- don't use ANY dns query type, some servers refuse those, use specific ones instead
- support multiline TXT records

## [0.2.1] 2025-04-08

### Fixed
- don't panic on wrong user inputs

## [0.2.0] 2025-03-25

### Added

- support for ``renepay``
- dynamic option ``payany-xpay-handle-pay`` as a replacement for ``xpay-handle-pay`` because only one plugin is allowed to modify rpc commands at a time
- dynamic option ``payany-budget-per``: rolling time interval for the budget, see Documentation for more info
- dynamic option ``payany-budget-amount-msat``: budget in msat allowed to be spent in ``payany-budget-per`` time interval, see Documentation for more info
- dynamic option ``payany-strict-lnurl``: strictly adhere to LUD-06 and LUD-16 and throw errors on missing/wrong metadata or description/hashes. Default is ``false``

### Changed

- don't require (but allow) ``amount_msat`` for offers that have a specific amount set
- set ``xpay-handle-pay`` to ``false`` on ``payany`` startup, this is necessary so there are no random conflicts between ``xpay`` and ``payany`` rewriting rpc commands. See ``payany-xpay-handle-pay`` if you want this functionality
- for devs: strip URI schemes more explicitly to support ports in URL's for local testing (also use http if URL contains ``localhost`` or ``127.0.0.1``)

### Fixed

- return error when trying to pay non-BTC offers, they are not supported, please fetch the invoice yourself

## [0.1.0] 2025-01-14

### Added

- initial release featuring automatic invoice fetching for offers, bip353 addresses (lightning only), lnurl and ln-addresses

