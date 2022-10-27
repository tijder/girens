#!/bin/bash

version=$(fgrep "version: " ../meson.build | grep -v "meson" | grep -o "'.*'" | sed "s/'//g")

find ../src -iname "*.py" | xargs xgettext --package-name=Girens --package-version=$version --from-code=UTF-8 --output=girens-python.pot
find ../data/ui -iname "*.ui" -or -iname "*.xml" -or -iname "*.ui.in" | xargs xgettext --package-name=Girens --package-version=$version --from-code=UTF-8 --output=girens-glade.pot -L Glade
find ../data/ui -iname "*.blp" | xargs xgettext --package-name=Girens --package-version=$version --output=girens-blueprint.pot --from-code=UTF-8 --add-comments --keyword=_ --keyword=C_:1c,2
find ../data/ -iname "*.desktop.in" | xargs xgettext --package-name=Girens --package-version=$version --from-code=UTF-8 --output=girens-desktop.pot -L Desktop
find ../data/ -iname "*.appdata.xml.in" | xargs xgettext --no-wrap --package-name=Girens --package-version=$version --from-code=UTF-8 --output=girens-appdata.pot

msgcat --sort-by-file --use-first --output-file=girens.pot girens-python.pot girens-glade.pot girens-blueprint.pot girens-desktop.pot girens-appdata.pot

sed 's/#: //g;s/:[0-9]*//g;s/\.\.\///g' <(fgrep "#: " girens.pot) | sed s/\ /\\n/ | sort | uniq > POTFILES.in

echo "# Please keep this list alphabetically sorted" > LINGUAS
for l in $(ls *.po); do basename $l .po >> LINGUAS; done

for lang in $(sed "s/^#.*$//g" LINGUAS); do
    mv "${lang}.po" "${lang}.po.old"
    msginit --no-translator --locale=$lang --input girens.pot
    mv "${lang}.po" "${lang}.po.new"
    msgmerge -N "${lang}.po.old" "${lang}.po.new" > ${lang}.po
    rm "${lang}.po.old" "${lang}.po.new"
done

rm girens-*.pot

# To create language file use this command
# msginit --locale=LOCALE --input girens.pot
# where LOCALE is something like `de`, `it`, `es`...

# To compile a .po file
# msgfmt --output-file=xx.mo xx.po
# where xx is something like `de`, `it`, `es`...
