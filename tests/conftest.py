from pyln.client import NodeVersion
import pytest
import subprocess


def get_cln_version():
    cln_version_proc = subprocess.check_output(["lightningd", "--version"])
    cln_version = NodeVersion(cln_version_proc.decode("ascii").strip())

    return cln_version


def pytest_configure(config):
    if not hasattr(config, "workerinput"):
        config.cln_version = get_cln_version()


def pytest_configure_node(node):
    node.workerinput["pay_renepay_deprecated"] = node.config.cln_version >= NodeVersion(
        "v26.06"
    )


@pytest.fixture(scope="session")
def pay_renepay_deprecated(request):
    if hasattr(request.config, "workerinput"):
        return request.config.workerinput["pay_renepay_deprecated"]

    return request.config.pay_renepay_deprecated
