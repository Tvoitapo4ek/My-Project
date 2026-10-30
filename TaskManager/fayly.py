# fayly.py
# тут всё, что связано с сохранением и загрузкой задач из файлов
# базы данных не используем - обычный json файл прекрасно справляется
# функции ничего не печатают сами, а возвращают текст сообщения -
# так их можно вызывать и из консоли (там print), и из окна (там всплывашка)

import json
import os
import shutil
from datetime import datetime
import constants


def zagruzit_zadachi():
    # загружаем задачи из json файла, если файла нет - возвращаем пустой список
    if not os.path.exists(constants.IMYA_FAYLA):
        return []
    try:
        with open(constants.IMYA_FAYLA, "r", encoding="utf-8") as fayl:
            dannye = json.load(fayl)
            return dannye
    except (json.JSONDecodeError, UnicodeDecodeError):
        print("Не получилось прочитать файл с задачами, начинаем с пустого списка.")
        return []


def sohranit_zadachi(spisok_zadach):
    with open(constants.IMYA_FAYLA, "w", encoding="utf-8") as fayl:
        json.dump(spisok_zadach, fayl, ensure_ascii=False, indent=4)


def eksport_v_tekstovyy_fayl(spisok_zadach):
    if len(spisok_zadach) == 0:
        return "Список задач пуст, экспортировать нечего."

    with open(constants.IMYA_TXT_FAYLA, "w", encoding="utf-8") as fayl:
        fayl.write("СПИСОК ДЕЛ\n")
        fayl.write("=" * 40 + "\n\n")
        for z in spisok_zadach:
            status = "Выполнено" if z["vypolneno"] else "Не выполнено"
            fayl.write(f"Задача №{z['id']}: {z['nazvanie']}\n")
            fayl.write(f"Статус: {status}\n")
            fayl.write(f"Категория: {z['kategoriya']}\n")
            fayl.write(f"Приоритет: {z['prioritet']}\n")
            srok = z["srok"] if z["srok"] else "не указан"
            fayl.write(f"Срок: {srok}\n")
            if z["opisanie"]:
                fayl.write(f"Описание: {z['opisanie']}\n")
            fayl.write("-" * 40 + "\n")

    return f"Задачи экспортированы в файл {constants.IMYA_TXT_FAYLA}"


def sozdat_rezervnuyu_kopiyu():
    if not os.path.exists(constants.IMYA_FAYLA):
        return "Файл с задачами еще не создан, нечего копировать."

    if not os.path.exists(constants.PAPKA_BACKUP):
        os.makedirs(constants.PAPKA_BACKUP)

    vremya = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
    imya_kopii = os.path.join(constants.PAPKA_BACKUP, f"zadachi_backup_{vremya}.json")
    shutil.copy(constants.IMYA_FAYLA, imya_kopii)
    return f"Резервная копия создана: {imya_kopii}"
