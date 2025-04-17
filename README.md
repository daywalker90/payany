[![latest release on CLN v25.02](https://github.com/daywalker90/payany/actions/workflows/latest_v25.02.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v25.02.yml) [![latest release on CLN v24.11.1](https://github.com/daywalker90/payany/actions/workflows/latest_v24.11.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v24.11.yml) [![latest release on CLN v24.08.2](https://github.com/daywalker90/payany/actions/workflows/latest_v24.08.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/latest_v24.08.yml)

[![main on CLN v25.02](https://github.com/daywalker90/payany/actions/workflows/main_v25.02.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v25.02.yml) [![main on CLN v24.11.1](https://github.com/daywalker90/payany/actions/workflows/main_v24.11.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v24.11.yml) [![main on CLN v24.08.2](https://github.com/daywalker90/payany/actions/workflows/main_v24.08.yml/badge.svg?branch=main)](https://github.com/daywalker90/payany/actions/workflows/main_v24.08.yml)

Failures in CLN 24.08.2 are a memleak error from CLN on shutdown.

# payany
A [CLN](https://github.com/ElementsProject/lightning) plugin to supercharge CLN's **pay**/**xpay**/**renepay**. It can automatically fetch invoices for all the static lightning payment addresses out there.You can also set a budget for how much **pay**/**xpay**/**renepay** can spend in a specific time window.

If you want to receive via LNURL/ln-address with optional Zap support check out [clnaddress](https://github.com/daywalker90/clnaddress)

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

Using **payany** may cause clearnet connections to fetch the invoices. DNS lookups for bip353 addresses use Google's DNS by default.

When using **pay**/**xpay**/**renepay** combined with **payany** and lightning payment methods that don't have a specific **amount_msat** set you are required to set the **amount_msat** argument in **pay**/**xpay**/**renepay**. This is for fetching/checking the invoice against your intended **amount_msat** to pay. **payany** also adds a new argument to **pay**/**xpay**/**renepay** called **message** (at the last position). It is an optional message you intend to send to the payee. This is either put in the **comment** field for LNURL based methods or in the **payer_note** for bolt12 based methods.

Using **pay**/**xpay**/**renepay** with **payany** enables you to use them like this:

`lightning-cli xpay user@domain.com 10000`

`lightning-cli pay user@domain.com 10000`

`lightning-cli renepay user@domain.com 10000`

When using the **message** argument it's usually easier to use the key=value format since **message** is in the last position of all the arguments of **pay**/**xpay**/**renepay** (and there are also **dev_** arguments not listed in the documentation!):

`lightning-cli xpay invstring=user@domain.com amount_msat=10000 message="thanks for the item"`

`lightning-cli pay bolt11=user@domain.com amount_msat=10000 message="thanks for the item"`

`lightning-cli renepay invstring=user@domain.com amount_msat=10000 message="thanks for the item"`

:warning:**payany** will set ``xpay-handle-pay`` to ``false``, see ``payany-xpay-handle-pay`` option

## Options

The budget options will prevent payment commands shipped with CLN (**pay**/**xpay**/**renepay**) to not exceed a certain budget in a certain time window. Only **pay**/**xpay**/**renepay** are being checked against the budget you can set. ``withdraw`` or ``fundchannel`` with ``push_msat`` shenanigans are **NOT** checked. These options are intendend to be used in combination with a rune similar to this:

``lightning-cli createrune -k restrictions='[["method^list", "method^get", "method=summary", "method=sql", "method=decode", "method=fetchinvoice", "method=pay", "method=xpay", "method=renepay"],["method/listdatastore"]]'`` 

You must also **NOT** allow ``setconfig`` since you can dynamically adjust the budget options during runtime. I would also use ``important-plugin=/path/to/payany`` to load it. 

- ``payany-budget-per`` If you want to set a budget for payments this is the rolling time window in which all payments (including fees, excluding self-payments) will be summed up and compared to ``payany-budget-amount-msat``. Valid time units are: ``seconds``, ``minutes``, ``hours``, ``days``, ``weeks`` and various abbreviations of them. Default is not set (unrestricted spending)
- ``payany-budget-amount-msat`` If you want to set a budget for payments this is the amount in msat (including fees, excluding self-payments) you want to be able to spend in your rolling time window set by ``payany-budget-per``. Default is not set (unrestricted spending)

Example if you want your node to only be able to spend 100.000 sats per week: ``payany-budget-per=1week`` and ``payany-budget-amount-msat=100000000``

- ``payany-xpay-handle-pay`` If you want to let ``xpay`` handle ``pay`` you would usually set ``xpay-handle-pay`` but only one plugin is allowed to modify rpc commands so ``payany`` has to take over this job since it is already modifying rpc commands to both ``pay`` and ``xpay`` when fetching invoices for static lightning payment addresses. Default is `false`

- ``payany-dns`` The DNS server to be used for the bip353 lookups and DNSSEC verification. You can choose from ``google``, ``cloudflare``, ``quad9``, ``system`` where ``system`` is the DNS of your operating system. Default is ``google``

- ``payany-strict-lnurl`` Adhere strictly to ``LUD-06`` and ``LUD-16`` (concerning metadata checks and description/hash checks). Mostly for testing. Since alot of big lnurl services don't do this, this mode is disabled by default so you will not get an error and instead a log entry. Default is ``false``

## Supported static lightning payment addresses:

- [bolt12](https://github.com/lightning/bolts/blob/master/12-offer-encoding.md) offers
- [BIP353](https://github.com/bitcoin/bips/blob/master/bip-0353.mediawiki) lightning addresses (DNAME DNS entries and non-ASCII identifiers not supported for now)
- LNURL lightning addresses and strings: [LUD-06](https://github.com/lnurl/luds/blob/luds/06.md), [LUD-12](https://github.com/lnurl/luds/blob/luds/12.md), [LUD-16](https://github.com/lnurl/luds/blob/luds/16.md)


## Methods
You can use this command to only fetch the invoice and not pay it directly:
* **payany** *invstring* *amount_msat* [*message*]
    * returns the *invoice* for an offer, bip353 ln-address, bech32-encoded LNURLP or LNURL-based ln-address
    * ***invstring***: the address you want to pay e.g. `user@domain.com` or `LNURL1DP6[..]6C72PP7X`
    * ***amount_msat***: the amount in msat you intend to pay. Always required for safety checks.
    * ***message***: an optional message you intend to send to the payee. This is either put in the *comment* field for LNURL based methods or in the *payer_note* for bolt12 based methods.

