#!/usr/bin/python3

# Copyright (c) 2025 Philip Taylor

# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:

# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.

# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

import yaml
import re

#OUTPUT_DIR = "posts"
OUTPUT_DIR = "../../web/zaynar/site/content/space"

# Combine dates from the scanned pages, to give a date (or range) for the combined web pages
def merge_dates(ds):
    if len(ds) == 0:
        return 'Unknown'
    if len(ds) == 1:
        return ds[0]
    if ds[0] == ds[1]:
        return ds[0]
    if ds[0] == '':
        return ds[1]
    if ds == ['Aug 1961', 'Aug 1961; Apr 1962?']:
        return 'Aug 1961; Apr 1962?'
    if ds == ['Aug 1961', '1961']:
        return '1961'
    if ds == ['Nov 1961', 'Oct--Nov 1961']:
        return 'Oct--Nov 1961'
    if ds == ['Nov 1961', 'Dec? 1961--Jan 1962']:
        return 'Nov 1961--Jan 1962'
    if ds == ['Jan 1962', '1962']:
        return '1962'
    if ds == ['Apr--May 1962', '1962']:
        return '1962'
    if ds == ['1962', '1961--1962']:
        return '1961--1962'
    if ds == ['1962', '1962?']:
        return '1962?'
    raise Exception(ds)

# Add an extra level of headings
def format_text(s):
    s = re.sub(r'^#', '##', s, flags = re.MULTILINE)
    return s

def escape_str(s):
    return s.translate(str.maketrans({
        "\n":  r"\n",
        "\r":  r"\r",
        '"': "\\\"",
        "\\":  "\\\\",
    }))

def expand_dashes(s):
    return s.replace("---", "—").replace("--", "–")

def format_page_num(n):
    if type(n) == int:
        return f"{n:02d}"
    else:
        return n

raw_articles = []

# Basic design:
#
# One post corresponds to one scanned image.
# One scanned image may have been split into ~2 pages and annotated separately.
#
# annotations.yaml contains the annotations for each page (indexed by the .jpg of that page).
# pages.yaml contains a list of scanned images and corresponding pages (without the .jpg suffix).
# The scanned image filename will be computed as the page names (without the .jpg) joined with "-".

def prepare_page(num, pagenum, page, annotations):
    if 'pages' in page:
        pagename = "-".join(format_page_num(p) for p in page["pages"])
        # if page["pages"][0] == "front":
        #     weight = 10000 * num + 999
        # else:
        #     weight = 10000 * num + 1000 + page["pages"][0]
        date = []
        summary = []
        for p in page.get('pages', []):
            ann = annotations['pages'].get(format_page_num(p) + ".jpg", None)
            if ann:
                date.append(ann["date"])
                for s in ann["summary"].split("; "):
                    if s not in summary:
                        summary.append(s)
        summary = ', '.join(summary)
        date = merge_dates(date)
        pagenums = ('Pages' if len(page['pages']) > 1 else 'Page') + ' ' + ', '.join(str(p) for p in page['pages'])
    elif 'front' in page["img"]:
        pagename = 'front'
        # weight = 10000 * num + 999
        date = 'N/A'
        summary = 'Front cover'
        pagenums = 'Front cover'
    elif 'back' in page["img"]:
        pagename = 'back'
        # weight = 10000 * num + 2000
        date = 'N/A'
        summary = 'Back cover'
        pagenums = 'Back cover'
    weight = 10000 * num + 1000 + pagenum
    fn = f'{OUTPUT_DIR}/scrapbook-{num}-{pagename}.md'
    print(fn, date, summary)

    if date == 'N/A' or date == '':
        title = expand_dashes(summary)
    else:
        title = expand_dashes(f'{date} --- {summary}')

    return {
        'page': page,
        'pagename': pagename,
        'weight': weight,
        'date': date,
        'summary': summary,
        'pagenums': pagenums,
        'fn': fn,
        'title': title,
    }

def convert_scrapbook(num, pages, annotations):

    pagedata = [ prepare_page(num, i, page, annotations) for (i, page) in enumerate(pages) ]

    for i,page in enumerate(pagedata):
        with open(page['fn'], 'w') as out:

            # if page['date'] == 'N/A' or page['date'] == '':
            #     date_summary = page["summary"]
            # else:
            #     date_summary = f'{page["date"]} --- {page["summary"]}'

            out.write(f'''+++
# Generated by create-posts.py
title = "Scrapbook {num}: {escape_str(page['title'])}"
description = "Space newspaper clippings. {escape_str(page['summary'])}"
weight = {page['weight']}
[extra]
og_image = "https://zaynar.co.uk/images/scrapbook-{num}/{page['pagename']}.jpg"
+++
''')

            navs = []
            if i > 0:
                navs.append(f'[Prev](@/space/scrapbook-{num}-{pagedata[i-1]["pagename"]}.md)')
            else:
                navs.append(f'Prev')
            navs.append(f"[Index](@/space/scrapbook-{num}-index.md)")
            if i + 1 < len(pagedata):
                navs.append(f'[Next](@/space/scrapbook-{num}-{pagedata[i+1]["pagename"]}.md)')
            else:
                navs.append(f'Next')
            nav = " | ".join(navs) + "\n\n"

            out.write(nav)

            # out.write(f'## {date_summary}\n\n')

            out.write('<section class="scrapbook-page">\n\n')

            # imgs = page['page'].get('imgs', None) or [ page['page']['img'] ]
            # for img in imgs:
            #     out.write('{{ scrapbookphoto(src="' + img + '",photopage="scrapbook-' + str(num) + '") }}\n\n')
            out.write('{{ scrapbookphoto(src="' + page['pagename'] + '.jpg",photopage="scrapbook-' + str(num) + '") }}\n\n')

            for p in page['page'].get('pages', []):
                ann = annotations['pages'].get(format_page_num(p) + ".jpg", None)

                if ann:
                    for article in ann.get('articles', []):
                        if article['text'].startswith('[NOTE] '):
                            out.write('<section class="scrapbook-note">\n\n')
                            out.write('NOTE: ')
                            out.write(format_text(article['text'][7:].strip()))
                            out.write('\n\n')
                            out.write('</section>\n\n')
                        else:
                            out.write('<section class="scrapbook-article">\n\n')
                            out.write(format_text(article['text'].strip()))
                            out.write('\n\n')
                            out.write('</section>\n\n')
                            raw_articles.append(article['text'].strip())

            out.write('</section>\n\n')

            out.write(nav)

    with open(f'{OUTPUT_DIR}/scrapbook-{num}-index.md', 'w') as out:
        out.write(f'''+++
# Generated by create-posts.py
title = "Scrapbook {num}"
description = "Space newspaper clippings."
weight = {10000 * num}
+++
''')
        out.write('<section class=scrapbook-index-photos>\n')
        for page in pagedata:
            out.write('{{ scrapbookindexphoto(photopage="scrapbook-' + str(num) + '",pagename="scrapbook-' + str(num) + '-' + page['pagename'] + '",src="' + page['pagename'] + '.jpg",title=`' + page['title'] + '`) }}\n')
        out.write('</section>\n')

        # out.write('<p>Pages are shown with an approximate date, and a partial list of topics mentioned.</p>\n')

        out.write('<section class=scrapbook-index-text>\n\n')
        for page in pagedata:
            out.write(f'* [{page["title"]}](@/space/scrapbook-{num}-{page["pagename"]}.md)\n')
        out.write('\n</section>\n')

with open('annotations/pages.yaml') as pages:
    with open('annotations/annotations.yaml') as annotations:
        convert_scrapbook(1, yaml.safe_load(pages), yaml.safe_load(annotations))

with open('annotations/pages2.yaml') as pages:
    with open('annotations/annotations2.yaml') as annotations:
        convert_scrapbook(2, yaml.safe_load(pages), yaml.safe_load(annotations))

with open('annotations/pages3.yaml') as pages:
    with open('annotations/annotations3.yaml') as annotations:
        convert_scrapbook(3, yaml.safe_load(pages), yaml.safe_load(annotations))

with open('raw-articles.txt', 'w') as f:
    f.write('\n'.join(raw_articles))
print(f'Articles: {len(raw_articles)}')
