# gui.py
# окно программы. сам внешний вид окна я нарисовал в Qt Designer (файл forma.ui),
# потом командой  pyuic6 forma.ui -o forma_ui.py  он сам превратился в код.
# forma_ui.py руками не трогаем - он перезаписывается при каждой конвертации.
# а тут только "оживляем" окно: заполняем списки, подключаем кнопки к функциям
# из zadachi.py и fayly.py. своих классов тут нет, как и во всей программе -
# только функции и пара глобальных переменных

import sys
from datetime import datetime

from PyQt6 import QtCore, QtGui, QtWidgets

import constants
import fayly
import zadachi
from forma_ui import Ui_GlavnoeOkno

# глобальные переменные окна (заполняются в zapustit())
okno = None            # само окно
ui = None              # все кнопки, поля и т.д. из forma.ui (ui.knopkaDobavit и т.п.)
spisok_zadach = []     # тот же список словарей, что и в консольной версии
vybrannyy_id = None    # id задачи, которая сейчас выделена в таблице (None - ничего)

# цвета для приоритетов, как в консоли через colorama
CVETA_PRIORITETOV = {
    "высокий": "#d32f2f",
    "средний": "#ef8f00",
    "низкий": "#2e7d32"
}


# ---------------------------------------------------------------
# вспомогательное
# ---------------------------------------------------------------

def soobshenie(tekst, zagolovok="Список дел"):
    QtWidgets.QMessageBox.information(okno, zagolovok, tekst)


def sohranit_i_obnovit():
    # после любого изменения сразу пишем в json и перерисовываем таблицу
    fayly.sohranit_zadachi(spisok_zadach)
    obnovit_tablicu()


def zapolnit_spiski():
    # варианты в выпадающих списках берем из constants.py,
    # чтобы категории и приоритеты были в одном месте для консоли и окна
    ui.comboFiltrKategorii.addItem("Все категории")
    ui.comboFiltrKategorii.addItems(constants.KATEGORII)
    ui.comboFiltrStatusa.addItems(constants.FILTRY_STATUSA)
    ui.comboSortirovka.addItems(constants.SPOSOBY_SORTIROVKI)
    ui.comboKategoriya.addItems(constants.KATEGORII)
    ui.comboPrioritet.addItems(constants.PRIORITETY)
    ui.comboPrioritet.setCurrentText("средний")


# ---------------------------------------------------------------
# таблица с задачами
# ---------------------------------------------------------------

def poluchit_vidimye_zadachi():
    # применяем по очереди поиск, фильтры и сортировку - всё из zadachi.py
    rezultat = spisok_zadach

    slovo = ui.lineEditPoisk.text()
    if slovo.strip() != "":
        rezultat = zadachi.naiti_po_slovu(rezultat, slovo)

    if ui.comboFiltrKategorii.currentIndex() > 0:  # 0 - это "Все категории"
        rezultat = zadachi.otfiltrovat_po_kategorii(rezultat, ui.comboFiltrKategorii.currentText())

    status = ui.comboFiltrStatusa.currentText()
    if status == "Невыполненные":
        rezultat = [z for z in rezultat if not z["vypolneno"]]
    elif status == "Выполненные":
        rezultat = [z for z in rezultat if z["vypolneno"]]
    elif status == "Просроченные":
        rezultat = zadachi.naiti_prosrochennye(rezultat)

    return zadachi.otsortirovat(rezultat, ui.comboSortirovka.currentIndex())


def obnovit_tablicu():
    vidimye = poluchit_vidimye_zadachi()
    tablica = ui.tableZadachi

    # пока заполняем таблицу, выключаем сигналы, иначе на каждую строку
    # будет срабатывать "выбрана другая строка"
    tablica.blockSignals(True)
    tablica.setRowCount(0)
    tablica.setRowCount(len(vidimye))

    for nomer_stroki, z in enumerate(vidimye):
        prosrochena = zadachi.prosrochena_li(z)
        yacheyki = [
            str(z["id"]),
            "✔" if z["vypolneno"] else "",
            z["nazvanie"],
            z["kategoriya"],
            z["prioritet"],
            z["srok"] if z["srok"] else "—"
        ]
        for nomer_stolbca, tekst in enumerate(yacheyki):
            yacheyka = QtWidgets.QTableWidgetItem(tekst)
            # id задачи прячем внутрь ячейки, чтобы потом понять, какая задача выбрана.
            # номер строки для этого не годится - после сортировки он другой
            yacheyka.setData(QtCore.Qt.ItemDataRole.UserRole, z["id"])

            if nomer_stolbca == 4:
                yacheyka.setForeground(QtGui.QColor(CVETA_PRIORITETOV.get(z["prioritet"], "#000000")))
            if nomer_stolbca == 5 and prosrochena:
                yacheyka.setForeground(QtGui.QColor("#d32f2f"))
                yacheyka.setToolTip("Задача просрочена!")
            if z["vypolneno"] and nomer_stolbca == 2:
                shrift = yacheyka.font()
                shrift.setStrikeOut(True)  # выполненные задачи зачеркиваем
                yacheyka.setFont(shrift)
            tablica.setItem(nomer_stroki, nomer_stolbca, yacheyka)

        # если эта задача была выделена до обновления - выделяем ее снова
        if z["id"] == vybrannyy_id:
            tablica.selectRow(nomer_stroki)

    tablica.blockSignals(False)
    tablica.resizeColumnsToContents()
    # столбец с названием растягиваем на все свободное место
    tablica.horizontalHeader().setSectionResizeMode(2, QtWidgets.QHeaderView.ResizeMode.Stretch)

    # внизу окна пишем коротко статистику
    st = zadachi.poschitat_statistiku(spisok_zadach)
    ui.statusbar.showMessage(
        f"Всего: {st['vsego']}   |   Показано: {len(vidimye)}   |   "
        f"Выполнено: {st['vypolneno']}   |   Просрочено: {st['prosrocheno']}"
    )


def pri_vybore_stroki():
    # пользователь кликнул по строке - показываем задачу справа в форме
    global vybrannyy_id
    vydelennye = ui.tableZadachi.selectedItems()
    if len(vydelennye) == 0:
        return
    vybrannyy_id = vydelennye[0].data(QtCore.Qt.ItemDataRole.UserRole)
    zadacha = zadachi.naiti_zadachu_po_id(spisok_zadach, vybrannyy_id)
    if zadacha is None:
        return

    ui.lineEditNazvanie.setText(zadacha["nazvanie"])
    ui.plainTextOpisanie.setPlainText(zadacha["opisanie"])
    ui.comboKategoriya.setCurrentText(zadacha["kategoriya"])
    ui.comboPrioritet.setCurrentText(zadacha["prioritet"])

    est_srok = False
    if zadacha["srok"]:
        try:
            d = datetime.strptime(zadacha["srok"], constants.FORMAT_DATY)
            ui.dateEditSrok.setDate(QtCore.QDate(d.year, d.month, d.day))
            est_srok = True
        except ValueError:
            pass
    if not est_srok:
        ui.dateEditSrok.setDate(QtCore.QDate.currentDate())
    ui.checkBoxSrok.setChecked(est_srok)

    ui.labelSozdano.setText(zadacha["data_sozdaniya"])
    if zadacha["vypolneno"]:
        ui.labelStatus.setText("выполнено")
        ui.knopkaVypolneno.setText("Вернуть в работу ↩️")
    else:
        tekst = "просрочено!" if zadachi.prosrochena_li(zadacha) else "не выполнено"
        ui.labelStatus.setText(tekst)
        ui.knopkaVypolneno.setText("Выполнено ✅")


# ---------------------------------------------------------------
# форма справа
# ---------------------------------------------------------------

def ochistit_formu():
    global vybrannyy_id
    vybrannyy_id = None
    ui.tableZadachi.clearSelection()
    ui.lineEditNazvanie.clear()
    ui.plainTextOpisanie.clear()
    ui.comboKategoriya.setCurrentIndex(0)
    ui.comboPrioritet.setCurrentText("средний")
    ui.checkBoxSrok.setChecked(False)
    ui.dateEditSrok.setDate(QtCore.QDate.currentDate())
    ui.labelSozdano.setText("-")
    ui.labelStatus.setText("-")
    ui.knopkaVypolneno.setText("Выполнено ✅")
    ui.lineEditNazvanie.setFocus()


def prochitat_formu():
    # забираем данные из полей формы. если название пустое - ругаемся и возвращаем None
    nazvanie = ui.lineEditNazvanie.text().strip()
    if nazvanie == "":
        soobshenie("Поле «Название» не может быть пустым.")
        ui.lineEditNazvanie.setFocus()
        return None

    srok = ""
    if ui.checkBoxSrok.isChecked():
        srok = ui.dateEditSrok.date().toString("dd.MM.yyyy")  # то же что FORMAT_DATY

    return {
        "nazvanie": nazvanie,
        "opisanie": ui.plainTextOpisanie.toPlainText().strip(),
        "kategoriya": ui.comboKategoriya.currentText(),
        "prioritet": ui.comboPrioritet.currentText(),
        "srok": srok
    }


def vybrannaya_zadacha():
    # возвращает выбранную задачу или None (и тогда просит сначала выбрать)
    zadacha = None
    if vybrannyy_id is not None:
        zadacha = zadachi.naiti_zadachu_po_id(spisok_zadach, vybrannyy_id)
    if zadacha is None:
        soobshenie("Сначала выберите задачу в списке слева.")
    return zadacha


# ---------------------------------------------------------------
# кнопки
# ---------------------------------------------------------------

def dobavit_zadachu():
    global vybrannyy_id
    dannye = prochitat_formu()
    if dannye is None:
        return
    novaya = zadachi.sozdat_zadachu(spisok_zadach, dannye["nazvanie"], dannye["opisanie"],
                                    dannye["kategoriya"], dannye["prioritet"], dannye["srok"])
    vybrannyy_id = novaya["id"]
    sohranit_i_obnovit()
    pri_vybore_stroki()
    ui.statusbar.showMessage("Задача успешно добавлена!", 3000)


def sohranit_izmeneniya():
    zadacha = vybrannaya_zadacha()
    if zadacha is None:
        return
    dannye = prochitat_formu()
    if dannye is None:
        return
    zadacha.update(dannye)  # меняем в словаре сразу все поля из формы
    sohranit_i_obnovit()
    pri_vybore_stroki()
    ui.statusbar.showMessage("Задача обновлена!", 3000)


def pereklyuchit_vypolneno():
    zadacha = vybrannaya_zadacha()
    if zadacha is None:
        return
    zadacha["vypolneno"] = not zadacha["vypolneno"]
    sohranit_i_obnovit()
    pri_vybore_stroki()


def udalit_zadachu():
    zadacha = vybrannaya_zadacha()
    if zadacha is None:
        return
    otvet = QtWidgets.QMessageBox.question(okno, "Удаление задачи",
                                           f"Точно удалить задачу '{zadacha['nazvanie']}'?")
    if otvet == QtWidgets.QMessageBox.StandardButton.Yes:
        spisok_zadach.remove(zadacha)
        ochistit_formu()
        sohranit_i_obnovit()


def pokazat_statistiku():
    soobshenie(zadachi.statistika_tekstom(spisok_zadach), "Статистика")


def pokazat_prosrochennye():
    # просто включаем нужный фильтр, таблица сама обновится
    ui.comboFiltrStatusa.setCurrentText("Просроченные")
    if len(zadachi.naiti_prosrochennye(spisok_zadach)) == 0:
        soobshenie("Просроченных задач нет, отлично!")


def eksport():
    soobshenie(fayly.eksport_v_tekstovyy_fayl(spisok_zadach), "Экспорт")


def rezervnaya_kopiya():
    soobshenie(fayly.sozdat_rezervnuyu_kopiyu(), "Резервная копия")


def o_programme():
    soobshenie("«Список дел»\n\n"
               "Курсовой проект: планировщик задач на Python.\n"
               "Окно нарисовано в Qt Designer (forma.ui), логика - в zadachi.py и fayly.py.\n"
               f"Задачи хранятся в файле:\n{constants.IMYA_FAYLA}", "О программе")


def podklyuchit_knopki():
    # связываем кнопки/меню из forma.ui с функциями выше.
    # если в дизайнере добавить новую кнопку - ее надо подключить тут же
    ui.knopkaDobavit.clicked.connect(dobavit_zadachu)
    ui.knopkaSohranit.clicked.connect(sohranit_izmeneniya)
    ui.knopkaVypolneno.clicked.connect(pereklyuchit_vypolneno)
    ui.knopkaUdalit.clicked.connect(udalit_zadachu)
    ui.knopkaOchistit.clicked.connect(ochistit_formu)
    ui.knopkaStatistika.clicked.connect(pokazat_statistiku)
    ui.knopkaEksport.clicked.connect(eksport)
    ui.knopkaBackup.clicked.connect(rezervnaya_kopiya)

    ui.actionNovaya.triggered.connect(ochistit_formu)
    ui.actionStatistika.triggered.connect(pokazat_statistiku)
    ui.actionProsrochennye.triggered.connect(pokazat_prosrochennye)
    ui.actionEksport.triggered.connect(eksport)
    ui.actionBackup.triggered.connect(rezervnaya_kopiya)
    ui.actionVyhod.triggered.connect(okno.close)
    ui.actionOProgramme.triggered.connect(o_programme)

    # поиск, фильтры и сортировка срабатывают сразу, без кнопки
    ui.lineEditPoisk.textChanged.connect(obnovit_tablicu)
    ui.comboFiltrKategorii.currentIndexChanged.connect(obnovit_tablicu)
    ui.comboFiltrStatusa.currentIndexChanged.connect(obnovit_tablicu)
    ui.comboSortirovka.currentIndexChanged.connect(obnovit_tablicu)

    ui.tableZadachi.itemSelectionChanged.connect(pri_vybore_stroki)
    ui.checkBoxSrok.toggled.connect(ui.dateEditSrok.setEnabled)
    ui.lineEditNazvanie.returnPressed.connect(dobavit_zadachu)  # Enter в названии = добавить


def zapustit():
    global okno, ui, spisok_zadach
    prilozhenie = QtWidgets.QApplication(sys.argv)

    okno = QtWidgets.QMainWindow()
    ui = Ui_GlavnoeOkno()
    ui.setupUi(okno)  # тут создаются все элементы, которые нарисованы в дизайнере

    spisok_zadach = fayly.zagruzit_zadachi()
    zapolnit_spiski()
    podklyuchit_knopki()
    ochistit_formu()
    obnovit_tablicu()

    okno.show()
    return prilozhenie.exec()


if __name__ == "__main__":
    sys.exit(zapustit())
