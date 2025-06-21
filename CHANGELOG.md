# Changelog

## [0.3.0] Unreleased
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

