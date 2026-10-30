# helpers.py
# тут разные вспомогательные функции - ввод данных, печать меню и всякое такое

import os
from datetime import datetime
import constants

# пробуем подключить colorama для цветного текста в консоли
# если библиотеки нет - программа всё равно будет работать, просто без цвета
try:
    from colorama import init, Fore, Style
    init(autoreset=True)
    EST_COLORAMA = True
except ImportError:
    EST_COLORAMA = False


def ochistit_ekran():
    # очищаем консоль, для винды и линукса команды разные
    if os.name == "nt":
        os.system("cls")
    else:
        os.system("clear")


def pauza():
    input("\nНажмите Enter, чтобы продолжить...")


def vyvesti_zagolovok(tekst):
    # красивый заголовок для разделов меню
    if EST_COLORAMA:
        print(Fore.CYAN + Style.BRIGHT + "\n=== " + tekst + " ===")
    else:
        print("\n=== " + tekst + " ===")


def vyvesti_menyu():
    vyvesti_zagolovok("СПИСОК ДЕЛ")
    print("1.  Добавить задачу")
    print("2.  Показать все задачи")
    print("3.  Показать одну задачу")
    print("4.  Редактировать задачу")
    print("5.  Удалить задачу")
    print("6.  Отметить задачу выполненной")
    print("7.  Отметить задачу невыполненной")
    print("8.  Поиск задач")
    print("9.  Фильтр по категории")
    print("10. Сортировка задач")
    print("11. Статистика")
    print("12. Просроченные задачи")
    print("13. Экспорт в текстовый файл")
    print("14. Сделать резервную копию")
    print("0.  Выход")


def poluchit_chislo(soobshenie, min_znach=None, max_znach=None):
    # запрашиваем у пользователя число, пока не введет нормально
    while True:
        vvod = input(soobshenie)
        try:
            chislo = int(vvod)
        except ValueError:
            print("Ошибка! Нужно ввести целое число.")
            continue
        if min_znach is not None and chislo < min_znach:
            print(f"Число должно быть не меньше {min_znach}")
            continue
        if max_znach is not None and chislo > max_znach:
            print(f"Число должно быть не больше {max_znach}")
            continue
        return chislo


def poluchit_neputoi_tekst(soobshenie):
    # просим ввести текст, который не пустой (например название задачи)
    while True:
        tekst = input(soobshenie).strip()
        if tekst == "":
            print("Поле не может быть пустым, попробуйте еще раз.")
        else:
            return tekst


def poluchit_datu(soobshenie):
    # дата не обязательна, можно нажать Enter и пропустить
    while True:
        tekst = input(soobshenie + " (формат ДД.ММ.ГГГГ, Enter - пропустить): ").strip()
        if tekst == "":
            return ""
        try:
            datetime.strptime(tekst, constants.FORMAT_DATY)
            return tekst
        except ValueError:
            print("Неправильный формат даты, попробуйте еще раз (например 25.12.2026)")


def poluchit_prioritet():
    print("Выберите приоритет:")
    for i, p in enumerate(constants.PRIORITETY, start=1):
        print(f"  {i}. {p}")
    nomer = poluchit_chislo("Ваш выбор: ", 1, len(constants.PRIORITETY))
    return constants.PRIORITETY[nomer - 1]


def poluchit_kategoriyu():
    print("Выберите категорию:")
    for i, k in enumerate(constants.KATEGORII, start=1):
        print(f"  {i}. {k}")
    nomer = poluchit_chislo("Ваш выбор: ", 1, len(constants.KATEGORII))
    return constants.KATEGORII[nomer - 1]


def cvet_prioriteta(prioritet):
    # возвращает цвет для приоритета, если colorama установлена
    if not EST_COLORAMA:
        return "", ""
    if prioritet == "высокий":
        return Fore.RED, Style.RESET_ALL
    elif prioritet == "средний":
        return Fore.YELLOW, Style.RESET_ALL
    else:
        return Fore.GREEN, Style.RESET_ALL


def vyvesti_zadachu(zadacha):
    # печатаем одну задачу в удобном виде
    cvet, sbros = cvet_prioriteta(zadacha["prioritet"])
    status = "[X]" if zadacha["vypolneno"] else "[ ]"
    stroka_sroka = zadacha["srok"] if zadacha["srok"] else "не указан"

    print(f"{status} №{zadacha['id']} - {cvet}{zadacha['nazvanie']}{sbros}")
    print(f"    Категория: {zadacha['kategoriya']} | "
          f"Приоритет: {cvet}{zadacha['prioritet']}{sbros} | "
          f"Срок: {stroka_sroka}")
    if zadacha["opisanie"]:
        print(f"    Описание: {zadacha['opisanie']}")
    print(f"    Создано: {zadacha['data_sozdaniya']}")
