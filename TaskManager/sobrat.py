# sobrat.py
# сборка всей программы в ОДИН файл, который запускается без установленного питона
# и без pip install - библиотека PyQt6 упаковывается прямо внутрь.
#
# как пользоваться (один раз поставить инструменты):
#     pip install PyQt6 pyinstaller
#     python sobrat.py
# готовый файл появится в папке dist:
#     Windows - dist/SpisokDel.exe
#     macOS   - dist/SpisokDel.app (и dist/SpisokDel)
#     Linux   - dist/SpisokDel
#
# важно: pyinstaller собирает только под ту систему, на которой его запустили.
# то есть exe для винды собирается на винде, для мака - на маке.
# чтобы не искать три компьютера, есть файл .github/workflows/sborka.yml -
# гитхаб сам собирает все три версии (вкладка Actions в репозитории)

import os
import subprocess
import sys

PAPKA = os.path.dirname(os.path.abspath(__file__))


def main():
    os.chdir(PAPKA)

    # шаг 1: на всякий случай заново делаем код из forma.ui,
    # вдруг окно поменяли в дизайнере и забыли сконвертировать
    print("Конвертирую forma.ui -> forma_ui.py ...")
    subprocess.run([sys.executable, "-m", "PyQt6.uic.pyuic", "forma.ui", "-o", "forma_ui.py"], check=True)

    # шаг 2: собираем один файл
    try:
        import PyInstaller.__main__
    except ImportError:
        print("Не установлен pyinstaller. Установите: pip install pyinstaller")
        return

    print("Собираю программу, это займет пару минут...")
    PyInstaller.__main__.run([
        "main.py",
        "--name", "SpisokDel",
        "--onefile",    # всё в один файл
        "--windowed",   # без черного окна консоли
        "--noconfirm",
        "--clean",
        "--exclude-module", "tkinter",
    ])
    print("\nГотово! Программа лежит в папке dist")


if __name__ == "__main__":
    main()
