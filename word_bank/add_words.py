"""
Add words from a file to the word bank.
"""
import os
import config
from .file_connection import find_file


def add_words(
    fn: str,
    dst_dir: str = config.SORTED_FILES,
) -> bool:
    """
    Add words from a file to the word bank.
    :param fn: The file containing the words to add.
    If file is not in config.SOURCE_FILES, moves it there.
    :param dst_dir: The directory containing the word bank files.
    :return: True if the words were added successfully, False otherwise.
    """
    # Find if file is valid
    fn = find_file(fn)

    # If not, return False
    if fn is None:
        return False
    
    # read the new words
    with open(fn, 'r') as file:
        new_words = file.readlines()
        new_words = [word.strip().lower() for word in new_words]
    
    # If dst_dir does not exist, make it
    os.makedirs(dst_dir)

    # for each word length, add the new words to the existing words of that file
    for i in range(config.MIN_LENGTH, config.MAX_LENGTH + 1):
        src_fn = os.path.join(dst_dir, config.FN_FORMAT.format(letter_count=i))

        # if no previous file, add all words of length i
        if not os.path.exists(src_fn):
            src_words = {word for word in new_words if len(word) == i}
        
        # otherwise, only add non-duplicates
        else:
            with open(src_fn, 'r') as src_file:
                src_words = src_file.readlines()
            
            src_words = [word.strip().lower() for word in src_words]
            src_words += [word for word in new_words if len(word) == i]
            src_words = set(src_words)

        # write the new words to the file
        with open(src_fn, 'w') as src_file:
            for word in src_words:
                src_file.write(word + '\n')
    
    # if all goes well, return True
    return True


if __name__ == '__main__':
    add_words('./files/original/Collins Scrabble Words (2019).txt')