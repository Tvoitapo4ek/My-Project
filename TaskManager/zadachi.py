# zadachi.py
# основная логика работы с задачами - добавление, удаление, редактирование и т.д.
# базы данных и классы тут не используются - список задач это обычный список словарей
#
# файл поделен на две части:
#   1) "чистые" функции - они только считают и меняют список, ничего не спрашивают
#      и не печатают. их использует и окно программы (gui.py), и консоль
#   2) консольные функции - спрашивают данные через input() и печатают результат

from datetime import datetime
import helpers
import constants


# ---------------------------------------------------------------
# 1) ЧИСТЫЕ ФУНКЦИИ (общие для окна и для консоли)
# ---------------------------------------------------------------

def sgenerirovat_id(spisok_zadach):
    # придумываем новый id для задачи - берем максимальный существующий и +1
    if len(spisok_zadach) == 0:
        return 1
    maks_id = 0
    for z in spisok_zadach:
        if z["id"] > maks_id:
            maks_id = z["id"]
    return maks_id + 1


def naiti_zadachu_po_id(spisok_zadach, id_zadachi):
    # вспомогательная функция, ищет задачу в списке по номеру
    # если не находит - возвращает None
    for z in spisok_zadach:
        if z["id"] == id_zadachi:
            return z
    return None


def sozdat_zadachu(spisok_zadach, nazvanie, opisanie, kategoriya, prioritet, srok):
    # собираем словарь новой задачи и кладем его в список
    novaya_zadacha = {
        "id": sgenerirovat_id(spisok_zadach),
        "nazvanie": nazvanie,
        "opisanie": opisanie,
        "kategoriya": kategoriya,
        "prioritet": prioritet,
        "srok": srok,
        "vypolneno": False,
        "data_sozdaniya": datetime.now().strftime(constants.FORMAT_DATY)
    }
    spisok_zadach.append(novaya_zadacha)
    return novaya_zadacha


def naiti_po_slovu(spisok_zadach, slovo):
    # ищем слово в названии и описании, регистр не важен
    slovo = slovo.strip().lower()
    naideno = []
    for z in spisok_zadach:
        if slovo in z["nazvanie"].lower() or slovo in z["opisanie"].lower():
            naideno.append(z)
    return naideno


def otfiltrovat_po_kategorii(spisok_zadach, kategoriya):
    return [z for z in spisok_zadach if z["kategoriya"] == kategoriya]


def otsortirovat(spisok_zadach, sposob):
    # sposob: 0 - не сортируем, 1 - по приоритету, 2 - по названию, 3 - по статусу
    if sposob == 1:
        # приоритеты в списке идут от низкого к высокому, поэтому переворачиваем
        poryadok = {"низкий": 0, "средний": 1, "высокий": 2}
        return sorted(spisok_zadach, key=lambda z: poryadok[z["prioritet"]], reverse=True)
    elif sposob == 2:
        return sorted(spisok_zadach, key=lambda z: z["nazvanie"].lower())
    elif sposob == 3:
        return sorted(spisok_zadach, key=lambda z: z["vypolneno"])
    return list(spisok_zadach)


def prosrochena_li(zadacha):
    # задача просрочена, если срок есть, она не выполнена и срок уже прошел
    if not zadacha["srok"] or zadacha["vypolneno"]:
        return False
    try:
        data_sroka = datetime.strptime(zadacha["srok"], constants.FORMAT_DATY).date()
    except ValueError:
        return False
    # сравниваем именно даты, а не дату со временем, иначе задача со сроком
    # "сегодня" уже с утра считалась бы просроченной
    return data_sroka < datetime.now().date()


def naiti_prosrochennye(spisok_zadach):
    return [z for z in spisok_zadach if prosrochena_li(z)]


def poschitat_statistiku(spisok_zadach):
    # считаем всё сразу и возвращаем словарем, а печатать будет тот, кто вызвал
    vsego = len(spisok_zadach)
    vypolneno = 0
    for z in spisok_zadach:
        if z["vypolneno"]:
            vypolneno += 1

    po_kategoriyam = {}
    for kat in constants.KATEGORII:
        kolichestvo = 0
        for z in spisok_zadach:
            if z["kategoriya"] == kat:
                kolichestvo += 1
        if kolichestvo > 0:
            po_kategoriyam[kat] = kolichestvo

    po_prioritetam = {}
    for prior in constants.PRIORITETY:
        kolichestvo = 0
        for z in spisok_zadach:
            if z["prioritet"] == prior:
                kolichestvo += 1
        po_prioritetam[prior] = kolichestvo

    procent = round(vypolneno / vsego * 100, 1) if vsego > 0 else 0
    return {
        "vsego": vsego,
        "vypolneno": vypolneno,
        "nevypolneno": vsego - vypolneno,
        "procent": procent,
        "prosrocheno": len(naiti_prosrochennye(spisok_zadach)),
        "po_kategoriyam": po_kategoriyam,
        "po_prioritetam": po_prioritetam
    }


def statistika_tekstom(spisok_zadach):
    # та же статистика, но уже готовым текстом (для консоли и для окна)
    if len(spisok_zadach) == 0:
        return "Задач пока нет."
    st = poschitat_statistiku(spisok_zadach)
    stroki = [
        f"Всего задач: {st['vsego']}",
        f"Выполнено: {st['vypolneno']}",
        f"Не выполнено: {st['nevypolneno']}",
        f"Просрочено: {st['prosrocheno']}",
        f"Процент выполнения: {st['procent']}%",
        "",
        "По категориям:"
    ]
    for kat, kolichestvo in st["po_kategoriyam"].items():
        stroki.append(f"  {kat}: {kolichestvo}")
    stroki.append("")
    stroki.append("По приоритетам:")
    for prior, kolichestvo in st["po_prioritetam"].items():
        stroki.append(f"  {prior}: {kolichestvo}")
    return "\n".join(stroki)


# ---------------------------------------------------------------
# 2) КОНСОЛЬНЫЕ ФУНКЦИИ (меню в main.py --konsol)
# ---------------------------------------------------------------

def napechatat_spisok(spisok):
    for z in spisok:
        helpers.vyvesti_zadachu(z)
        print("-" * 40)


def dobavit_zadachu(spisok_zadach):
    helpers.vyvesti_zagolovok("Добавление новой задачи")
    nazvanie = helpers.poluchit_neputoi_tekst("Введите название задачи: ")
    opisanie = input("Введите описание (необязательно): ").strip()
    kategoriya = helpers.poluchit_kategoriyu()
    prioritet = helpers.poluchit_prioritet()
    srok = helpers.poluchit_datu("Введите срок выполнения")

    sozdat_zadachu(spisok_zadach, nazvanie, opisanie, kategoriya, prioritet, srok)
    print("\nЗадача успешно добавлена!")


def pokazat_vse_zadachi(spisok_zadach):
    helpers.vyvesti_zagolovok("Все задачи")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    napechatat_spisok(spisok_zadach)


def pokazat_odnu_zadachu(spisok_zadach):
    helpers.vyvesti_zagolovok("Просмотр задачи")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    id_zadachi = helpers.poluchit_chislo("Введите номер задачи: ")
    zadacha = naiti_zadachu_po_id(spisok_zadach, id_zadachi)
    if zadacha is None:
        print("Задача с таким номером не найдена.")
    else:
        helpers.vyvesti_zadachu(zadacha)


def redaktirovat_zadachu(spisok_zadach):
    helpers.vyvesti_zagolovok("Редактирование задачи")
    if len(spisok_zadach) == 0:
        print("Список задач пуст, нечего редактировать.")
        return
    id_zadachi = helpers.poluchit_chislo("Введите номер задачи для редактирования: ")
    zadacha = naiti_zadachu_po_id(spisok_zadach, id_zadachi)
    if zadacha is None:
        print("Задача с таким номером не найдена.")
        return

    print("Оставьте поле пустым (просто Enter), если не хотите его менять.")

    novoe_nazvanie = input(f"Название [{zadacha['nazvanie']}]: ").strip()
    if novoe_nazvanie != "":
        zadacha["nazvanie"] = novoe_nazvanie

    novoe_opisanie = input(f"Описание [{zadacha['opisanie']}]: ").strip()
    if novoe_opisanie != "":
        zadacha["opisanie"] = novoe_opisanie

    otvet = input("Изменить категорию? (да/нет): ").strip().lower()
    if otvet == "да":
        zadacha["kategoriya"] = helpers.poluchit_kategoriyu()

    otvet = input("Изменить приоритет? (да/нет): ").strip().lower()
    if otvet == "да":
        zadacha["prioritet"] = helpers.poluchit_prioritet()

    otvet = input("Изменить срок выполнения? (да/нет): ").strip().lower()
    if otvet == "да":
        zadacha["srok"] = helpers.poluchit_datu("Введите новый срок")

    print("\nЗадача обновлена!")


def udalit_zadachu(spisok_zadach):
    helpers.vyvesti_zagolovok("Удаление задачи")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    id_zadachi = helpers.poluchit_chislo("Введите номер задачи для удаления: ")
    zadacha = naiti_zadachu_po_id(spisok_zadach, id_zadachi)
    if zadacha is None:
        print("Задача с таким номером не найдена.")
        return
    podtverzhdenie = input(f"Точно удалить задачу '{zadacha['nazvanie']}'? (да/нет): ").strip().lower()
    if podtverzhdenie == "да":
        spisok_zadach.remove(zadacha)
        print("Задача удалена.")
    else:
        print("Удаление отменено.")


def otmetit_kak_vypolnennuyu(spisok_zadach):
    helpers.vyvesti_zagolovok("Отметить задачу как выполненную")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    id_zadachi = helpers.poluchit_chislo("Введите номер задачи: ")
    zadacha = naiti_zadachu_po_id(spisok_zadach, id_zadachi)
    if zadacha is None:
        print("Задача с таким номером не найдена.")
        return
    zadacha["vypolneno"] = True
    print("Задача отмечена как выполненная!")


def otmetit_kak_nevypolnennuyu(spisok_zadach):
    helpers.vyvesti_zagolovok("Отметить задачу как невыполненную")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    id_zadachi = helpers.poluchit_chislo("Введите номер задачи: ")
    zadacha = naiti_zadachu_po_id(spisok_zadach, id_zadachi)
    if zadacha is None:
        print("Задача с таким номером не найдена.")
        return
    zadacha["vypolneno"] = False
    print("Задача отмечена как невыполненная.")


def poisk_zadach(spisok_zadach):
    helpers.vyvesti_zagolovok("Поиск задач")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    slovo = input("Введите слово для поиска (по названию и описанию): ")
    naideno = naiti_po_slovu(spisok_zadach, slovo)

    if len(naideno) == 0:
        print("Ничего не найдено.")
    else:
        print(f"Найдено задач: {len(naideno)}")
        napechatat_spisok(naideno)


def filtr_po_kategorii(spisok_zadach):
    helpers.vyvesti_zagolovok("Фильтр по категории")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    kategoriya = helpers.poluchit_kategoriyu()
    otfiltrovano = otfiltrovat_po_kategorii(spisok_zadach, kategoriya)
    if len(otfiltrovano) == 0:
        print("В этой категории задач нет.")
    else:
        napechatat_spisok(otfiltrovano)


def sortirovka_zadach(spisok_zadach):
    helpers.vyvesti_zagolovok("Сортировка задач")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return
    print("1. По приоритету (сначала высокий)")
    print("2. По названию (по алфавиту)")
    print("3. По статусу (сначала невыполненные)")
    vybor = helpers.poluchit_chislo("Ваш выбор: ", 1, 3)
    napechatat_spisok(otsortirovat(spisok_zadach, vybor))


def pokazat_statistiku(spisok_zadach):
    helpers.vyvesti_zagolovok("Статистика")
    print(statistika_tekstom(spisok_zadach))


def pokazat_prosrochennye_zadachi(spisok_zadach):
    helpers.vyvesti_zagolovok("Просроченные задачи")
    if len(spisok_zadach) == 0:
        print("Список задач пуст.")
        return

    naideny = naiti_prosrochennye(spisok_zadach)
    if len(naideny) == 0:
        print("Просроченных задач нет, отлично!")
    else:
        print(f"Просроченных задач: {len(naideny)}")
        napechatat_spisok(naideny)
