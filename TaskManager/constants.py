# constants.py
# тут просто храним всякие настройки и списки, чтобы не разбрасывать их по всей программе

import os
import sys

# папка, где лежит программа. если программу собрали в один exe (pyinstaller),
# то __file__ указывает во временную папку, поэтому берем папку самого exe.
# так файлы с задачами всегда будут лежать рядом с программой, а не где попало
if getattr(sys, "frozen", False):
    PAPKA_PROGRAMMY = os.path.dirname(sys.executable)
else:
    PAPKA_PROGRAMMY = os.path.dirname(os.path.abspath(__file__))

IMYA_FAYLA = os.path.join(PAPKA_PROGRAMMY, "zadachi.json")            # в этот файл сохраняем список задач
IMYA_TXT_FAYLA = os.path.join(PAPKA_PROGRAMMY, "zadachi_export.txt")  # сюда выгружаем задачи в читаемом виде
PAPKA_BACKUP = os.path.join(PAPKA_PROGRAMMY, "backup")                # папка для резервных копий

# уровни приоритета, от простого к сложному
PRIORITETY = ["низкий", "средний", "высокий"]

# категории задач
KATEGORII = ["Учёба", "Работа", "Дом", "Личное", "Другое"]

# формат даты, который используем везде в программе
FORMAT_DATY = "%d.%m.%Y"

# способы сортировки (номера такие же, как в консольном меню)
SPOSOBY_SORTIROVKI = ["Без сортировки", "По приоритету", "По названию", "По статусу"]

# что можно показывать в списке в окне программы
FILTRY_STATUSA = ["Все задачи", "Невыполненные", "Выполненные", "Просроченные"]
