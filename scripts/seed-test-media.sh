#!/usr/bin/env bash
# Fill a folder with fake movies, shows and anime for testing Kinora libraries.
#
#   scripts/seed-test-media.sh                 # seeds ~/test-media
#   scripts/seed-test-media.sh /path/to/media  # seeds another folder
#   scripts/seed-test-media.sh --clean         # removes only what this script made
#
# Every file is a hard link to one 2-second video (about 20 KB), so hundreds of
# titles take almost no disk space, yet they still probe and play like real files.
# Titles are real so metadata lookups find posters. Files that already exist are
# left alone, so it's safe to run again. Needs ffmpeg (macOS: `brew install ffmpeg`).
#
# Layout (what Kinora's scanner expects):
#   Movies/Title (Year)/Title (Year).mkv
#   Shows/Show (Year)/Season 01/Show - S01E01.mkv
#   Anime/Show (Year)/Season 01/Show - S01E01.mkv
#
# Works with the bash 3.2 that ships with macOS.

set -euo pipefail

ROOT="$HOME/test-media"
CLEAN=0
for arg in "$@"; do
  case "$arg" in
    --clean) CLEAN=1 ;;
    -h | --help)
      sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
    *) ROOT="$arg" ;;
  esac
done

MANIFEST="$ROOT/.kinora-seed-manifest"
SEED_DIR="$ROOT/.kinora-seed"
SEED="$SEED_DIR/seed.mkv"

if [ "$CLEAN" = 1 ]; then
  if [ ! -f "$MANIFEST" ]; then
    echo "Nothing to clean in $ROOT (no $MANIFEST)."
    exit 0
  fi
  removed=0
  while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    if [ -f "$ROOT/$rel" ]; then
      rm -f "$ROOT/$rel"
      removed=$((removed + 1))
    fi
    # Remove the folders it lived in once they're empty (never the library roots).
    dir=$(dirname "$ROOT/$rel")
    while [ "$dir" != "$ROOT" ] && [ "$(dirname "$dir")" != "$ROOT" ]; do
      rmdir "$dir" 2>/dev/null || break
      dir=$(dirname "$dir")
    done
  done <"$MANIFEST"
  rm -rf "$SEED_DIR" "$MANIFEST"
  echo "Removed $removed seeded files from $ROOT."
  exit 0
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found. On macOS: brew install ffmpeg" >&2
  exit 1
fi

mkdir -p "$ROOT/Movies" "$ROOT/Shows" "$ROOT/Anime" "$SEED_DIR"
touch "$MANIFEST"

if [ ! -s "$SEED" ]; then
  echo "Making the seed video..."
  ffmpeg -loglevel error -y \
    -f lavfi -i "testsrc=size=320x180:rate=24" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000" \
    -t 2 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -b:a 64k -shortest "$SEED"
fi

made=0
# place <path relative to ROOT>
place() {
  local rel="$1" dest="$ROOT/$1"
  [ -e "$dest" ] && return 0
  mkdir -p "$(dirname "$dest")"
  # Hard link (no extra space); an APFS clone (also no extra space); a plain copy as a last resort.
  ln "$SEED" "$dest" 2>/dev/null || cp -c "$SEED" "$dest" 2>/dev/null || cp "$SEED" "$dest"
  echo "$rel" >>"$MANIFEST"
  made=$((made + 1))
}

# Movies: "Title|Year". A mix that exercises the A–Z rail: numbers (#), leading
# "The"/"A"/"An", accents, and a letter with nothing (X).
while IFS='|' read -r title year; do
  [ -n "$title" ] || continue
  place "Movies/$title ($year)/$title ($year).mkv"
done <<'EOF'
10 Things I Hate About You|1999
12 Angry Men|1957
1917|2019
2001 A Space Odyssey|1968
28 Days Later|2002
300|2006
A Beautiful Mind|2001
A Clockwork Orange|1971
A Quiet Place|2018
About Time|2013
Alien|1979
Aliens|1986
Amélie|2001
American Beauty|1999
An American Werewolf in London|1981
Apocalypse Now|1979
Arrival|2016
Back to the Future|1985
Baby Driver|2017
Barbie|2023
Before Sunrise|1995
Birdman|2014
Black Swan|2010
Blade Runner|1982
Blade Runner 2049|2017
Casablanca|1942
Casino Royale|2006
Catch Me If You Can|2002
Children of Men|2006
Chinatown|1974
Coco|2017
Collateral|2004
Dune|2021
Dune Part Two|2024
Drive|2011
Die Hard|1988
Django Unchained|2012
Edge of Tomorrow|2014
Eternal Sunshine of the Spotless Mind|2004
Everything Everywhere All at Once|2022
Ex Machina|2014
Fargo|1996
Fight Club|1999
Finding Nemo|2003
Forrest Gump|1994
Get Out|2017
Gladiator|2000
Gone Girl|2014
Goodfellas|1990
Gravity|2013
Groundhog Day|1993
Heat|1995
Her|2013
Hereditary|2018
Hot Fuzz|2007
Inception|2010
Inglourious Basterds|2009
Interstellar|2014
It Follows|2014
Jaws|1975
John Wick|2014
Jurassic Park|1993
Juno|2007
Kill Bill Vol 1|2003
Knives Out|2019
Kingdom of Heaven|2005
La La Land|2016
Lady Bird|2017
Logan|2017
Lost in Translation|2003
Mad Max Fury Road|2015
Memento|2000
Moonlight|2016
Mulholland Drive|2001
No Country for Old Men|2007
Nightcrawler|2014
Nope|2022
Oldboy|2003
Oppenheimer|2023
Ocean's Eleven|2001
Paddington 2|2017
Parasite|2019
Pulp Fiction|1994
Prisoners|2013
Ratatouille|2007
Reservoir Dogs|1992
Rocky|1976
Rear Window|1954
Se7en|1995
Shutter Island|2010
Sicario|2015
Spirited Away|2001
Taxi Driver|1976
The Dark Knight|2008
The Departed|2006
The Godfather|1972
The Grand Budapest Hotel|2014
The Matrix|1999
The Prestige|2006
The Shawshank Redemption|1994
The Social Network|2010
The Thing|1982
Titanic|1997
Toy Story|1995
Up|2009
Us|2019
Uncut Gems|2019
Vertigo|1958
V for Vendetta|2005
WALL-E|2008
Whiplash|2014
Wall Street|1987
Yesterday|2019
Young Frankenstein|1974
Zodiac|2007
Zootopia|2016
EOF

# Shows and anime: "Show|Year|Seasons|Episodes per season"
seed_series() {
  local lib="$1" show year seasons eps s e dir
  while IFS='|' read -r show year seasons eps; do
    [ -n "$show" ] || continue
    s=1
    while [ "$s" -le "$seasons" ]; do
      dir=$(printf "%s/%s (%s)/Season %02d" "$lib" "$show" "$year" "$s")
      e=1
      while [ "$e" -le "$eps" ]; do
        place "$(printf "%s/%s - S%02dE%02d.mkv" "$dir" "$show" "$s" "$e")"
        e=$((e + 1))
      done
      s=$((s + 1))
    done
  done
}

seed_series Shows <<'EOF'
Arrested Development|2003|2|6
Band of Brothers|2001|1|10
Better Call Saul|2015|2|6
Black Mirror|2011|2|3
Breaking Bad|2008|3|6
Chernobyl|2019|1|5
Dark|2017|2|6
Fleabag|2016|2|6
Game of Thrones|2011|2|6
Mr. Robot|2015|2|6
Severance|2022|2|6
Stranger Things|2016|2|6
Succession|2018|2|6
The Bear|2022|2|6
The Last of Us|2023|1|9
The Office|2005|3|6
The Wire|2002|2|6
True Detective|2014|1|8
Ted Lasso|2020|2|6
Westworld|2016|1|10
EOF

seed_series Anime <<'EOF'
Attack on Titan|2013|2|6
Bocchi the Rock!|2022|1|12
Chainsaw Man|2022|1|12
Cowboy Bebop|1998|1|12
Death Note|2006|1|12
Demon Slayer|2019|2|6
Frieren Beyond Journey's End|2023|1|12
Fullmetal Alchemist Brotherhood|2009|1|12
Jujutsu Kaisen|2020|2|6
Mob Psycho 100|2016|2|6
Neon Genesis Evangelion|1995|1|12
One Punch Man|2015|2|6
Spy x Family|2022|2|6
Steins;Gate|2011|1|12
Vinland Saga|2019|2|6
EOF

size=$(du -sh "$ROOT" 2>/dev/null | cut -f1)
echo "Added $made files to $ROOT (folder is now $size on disk)."
echo "In Kinora, add libraries for $ROOT/Movies (Movies), $ROOT/Shows (TV) and $ROOT/Anime (TV), or rescan them."
