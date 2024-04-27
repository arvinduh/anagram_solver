from string import Template 

# File structure
SOURCE_FILES = './files/original/'
SORTED_FILES = './files/sorted/'
FN_FORMAT = '{letter_count}_letter_words.txt'

# Default params
MIN_LENGTH = 3
MAX_LENGTH = 8
DFLT_MAX_LETTERS = 6

# sanity checks
assert MIN_LENGTH > 0, f'MIN_LENGTH ({MIN_LENGTH}) must be greater than 0'
assert MAX_LENGTH >= MIN_LENGTH, f'MAX_LENGTH ({MAX_LENGTH}) must be greater or equal to MIN_LENGTH ({MIN_LENGTH})'
assert DFLT_MAX_LETTERS <= MAX_LENGTH, f'DFLT_MAX_LETTERS ({DFLT_MAX_LETTERS}) must be less than or equal to MAX_LENGTH ({MAX_LENGTH})'
assert DFLT_MAX_LETTERS >= MIN_LENGTH, f'DFLT_MAX_LETTERS ({DFLT_MAX_LETTERS}) must be greater than or equal to MIN_LENGTH ({MIN_LENGTH})'