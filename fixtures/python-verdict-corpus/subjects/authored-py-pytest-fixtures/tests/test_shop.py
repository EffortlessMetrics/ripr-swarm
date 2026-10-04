from shop import greeting, handling_fee, save_report, warn


def test_deposit_through_fixture(account):
    account.deposit(50)
    assert account.balance == 150


def test_handling_fee_boundary(fee_case):
    amount, fee = fee_case
    assert handling_fee(amount) == fee


def test_report_file(tmp_path):
    report = tmp_path / "report.txt"
    save_report(report, 5)
    assert report.read_text() == "total=5\n"


def test_greeting_uses_environment(monkeypatch):
    monkeypatch.setenv("USER_NAME", "ann")
    assert greeting() == "hello ann"


def test_warning_goes_to_stderr(capsys):
    warn("low stock")
    assert capsys.readouterr().err == "warning: low stock\n"
