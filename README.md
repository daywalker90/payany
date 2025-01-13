[![latest release on CLN v24.11.1](https://github.com/daywalker90/payany/actions/workflows/latest_v24.11.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v24.11.yml) [![latest release on CLN v24.08.2](https://github.com/daywalker90/payany/actions/workflows/latest_v24.08.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v24.08.yml) [![latest release on CLN v24.05](https://github.com/daywalker90/payany/actions/workflows/latest_v24.05.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v24.05.yml)

[![main on CLN v24.11.1](https://github.com/daywalker90/payany/actions/workflows/main_v24.11.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v24.11.yml) [![main on CLN v24.08.2](https://github.com/daywalker90/payany/actions/workflows/main_v24.08.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v24.08.yml) [![main on CLN v24.05](https://github.com/daywalker90/payany/actions/workflows/main_v24.05.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v24.05.yml)

# payany
A core lightning plugin to supercharge CLN's xpay/pay to automatically fetch invoices for all the static lightning payment addresses out there.

- **[Installation](#installation)**
- **[Building](#building)**
- **[Documentation](#documentation)**

# Installation
For general plugin installation instructions see the plugins repo [README.md](https://github.com/lightningd/plugins/blob/master/README.md#Installation)

Release binaries for
* x86_64-linux
* armv7-linux (Raspberry Pi 32bit)
* aarch64-linux (Raspberry Pi 64bit)

can be found on the [release](https://github.com/daywalker90/payany/releases) page. If you are unsure about your architecture you can run ``uname -m``.

They require ``glibc>=2.31``, which you can check with ``ldd --version``.

# Building
You can build the plugin yourself instead of using the release binaries.
First clone the repo:

```
git clone https://github.com/daywalker90/payany.git
```

Install a recent rust version ([rustup](https://rustup.rs/) is recommended) and in the ``payany`` folder run:

```
cargo build --release
```

After that the binary will be here: ``target/release/payany``

Note: Release binaries are built using ``cross`` and the ``optimized`` profile.


# Documentation

Supported formats:

- [bolt12](https://github.com/lightning/bolts/blob/master/12-offer-encoding.md) offers
- [BIP353](https://github.com/bitcoin/bips/blob/master/bip-0353.mediawiki) lightning addresses (DNAME DNS entries and non-ASCII identifiers not supported for now)
- LNURL lightning addresses and strings: [LUD-06](https://github.com/lnurl/luds/blob/luds/06.md), [LUD-12](https://github.com/lnurl/luds/blob/luds/12.md), [LUD-16](https://github.com/lnurl/luds/blob/luds/16.md)

When using **pay** or **xpay** combined with **payany** and lightning payment methods that are supported by it it's always required to set the *amount_msat* argument, even if the method includes an amount. This is for checking the fetched invoice against your intended amount to pay. **payany** also adds a new argument to *pay* and *xpay* called *message* (at the last position). It is an optional message you intend to send to the payee. This is either put in the *comment* field for LNURL based methods or in the *payer_note* for bolt12 based methods.

Using *pay*/*xpay* with *payany* enables you to use them like this:

`lightning-cli xpay user@domain.com 10000`

`lightning-cli pay user@domain.com 10000`

When using the *message* argument it's usually easier to use the key=value format since *message* is in the last position of all the arguments of *pay*/*xpay*:

`lightning-cli xpay invstring=user@domain.com amount_msat=10000 message="thanks for the item"`

`lightning-cli pay bolt11=user@domain.com amount_msat=10000 message="thanks for the item"`

If you set `xpay-handle-pay=true` in CLN v24.11+ you will not see `payany`'s error's if you call *pay* with 2 or less arguments. I recommend using `xpay` directly or checking `payany`'s logs.

There is also a separate command if you don't want to immediately pay the invoice:
* **payany** *invstring* *amount_msat* [*message*]
    * returns the *invoice* for an offer, bip353 ln-address, bech32-encoded LNURLP or LNURL-based ln-address
    * ***invstring***: the address you want to pay e.g. `user@domain.com` or `LNURL1DP6[..]6C72PP7X`
    * ***amount_msat***: the amount in msat you intend to pay. Always required for safety checks.
    * ***message***: an optional message you intend to send to the payee. This is either put in the *comment* field for LNURL based methods or in the *payer_note* for bolt12 based methods.
