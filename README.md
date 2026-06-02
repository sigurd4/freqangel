# freqangel
A tool for doing things with SDRAngel frequency csv-files.

## Sorting

Sorts by frequency in ascending order.

```
freqangel sort am_frequencies.csv
```
## Dedup

Sorts by frequency and removes duplicate entires.

```
freqangel dedup am_frequencies.csv
```

## Merge

Merges all  by frequency and removes duplicate entires.

```
freqangel merge alternate_frequency_list.csv take temporary_frequency_list.csv into am_frequencies.csv
```

Frequency-lists given after the `take` keyword will be deleted after merging.

The last entry after the `into` keyword is the list which will be written to in the end.

## Fetch

Fetches an updated frequency-list from [`https://www1.s2.starcat.ne.jp/ndxc/`](https://www1.s2.starcat.ne.jp/ndxc/) and writes it to a given location.

```
freqangel fetch into userlist1.csv
```

It will only overwrite the file if the `into` keyword is given.