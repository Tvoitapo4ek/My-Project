# main.py
# главный файл программы - его и запускаем (F5 в IDLE или python main.py)
# Программа "Список дел" помогает планировать задачи по учебе, работе и дому.
# Без БД и без классов - все данные это обычный список словарей, который
# сохраняется в json файл между запусками программы.
#
# по умолчанию открывается окно (gui.py). старое консольное меню тоже осталось:
#     python main.py --konsol
# если библиотека PyQt6 не установлена, программа сама переключится на консоль

import sys
import helpers
import zadachi
import fayly


def konsolnoe_menyu():
    spisok_zadach = fayly.zagruzit_zadachi()

    print("Добро пожаловать в программу «Список дел»!")
    if not helpers.EST_COLORAMA:
        print("(Подсказка: установите библиотеку colorama - pip install colorama, будет красивее)")

    while True:
        helpers.vyvesti_menyu()
        vybor = helpers.poluchit_chislo("\nВведите номер пункта меню: ", 0, 14)

        if vybor == 1:
            zadachi.dobavit_zadachu(spisok_zadach)
            fayly.sohranit_zadachi(spisok_zadach)
        elif vybor == 2:
            zadachi.pokazat_vse_zadachi(spisok_zadach)
        elif vybor == 3:
            zadachi.pokazat_odnu_zadachu(spisok_zadach)
        elif vybor == 4:
            zadachi.redaktirovat_zadachu(spisok_zadach)
            fayly.sohranit_zadachi(spisok_zadach)
        elif vybor == 5:
            zadachi.udalit_zadachu(spisok_zadach)
            fayly.sohranit_zadachi(spisok_zadach)
        elif vybor == 6:
            zadachi.otmetit_kak_vypolnennuyu(spisok_zadach)
            fayly.sohranit_zadachi(spisok_zadach)
        elif vybor == 7:
            zadachi.otmetit_kak_nevypolnennuyu(spisok_zadach)
            fayly.sohranit_zadachi(spisok_zadach)
        elif vybor == 8:
            zadachi.poisk_zadach(spisok_zadach)
        elif vybor == 9:
            zadachi.filtr_po_kategorii(spisok_zadach)
        elif vybor == 10:
            zadachi.sortirovka_zadach(spisok_zadach)
        elif vybor == 11:
            zadachi.pokazat_statistiku(spisok_zadach)
        elif vybor == 12:
            zadachi.pokazat_prosrochennye_zadachi(spisok_zadach)
        elif vybor == 13:
            print(fayly.eksport_v_tekstovyy_fayl(spisok_zadach))
        elif vybor == 14:
            print(fayly.sozdat_rezervnuyu_kopiyu())
        elif vybor == 0:
            print("\nДо свидания! Все задачи сохранены.")
            break

        helpers.pauza()
        helpers.ochistit_ekran()


def main():
    if "--konsol" in sys.argv:
        konsolnoe_menyu()
        return

    # пробуем открыть окно. если PyQt6 нет - не падаем, а запускаем консоль
    try:
        import gui
    except ImportError:
        print("Библиотека PyQt6 не найдена, запускаю консольную версию.")
        print("(Для окна установите ее: pip install PyQt6)\n")
        konsolnoe_menyu()
        return
    sys.exit(gui.zapustit())


if __name__ == "__main__":
    main()
