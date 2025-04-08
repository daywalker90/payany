# Changelog

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

