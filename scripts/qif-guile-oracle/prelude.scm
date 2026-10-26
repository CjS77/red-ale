;; Stand-ins for the GnuCash modules qif-parse.scm loads, for run.sh. The two string helpers are copied from
;; 5.17:gnucash/import-export/qif-imp/string.scm:87-101 (GPL-2.0-or-later).
(use-modules (srfi srfi-1) (srfi srfi-13) (ice-9 regex))
(define (gnc:warn . args) #f)
(define (G_ s) s)
(define GNC-BANK-TYPE 0) (define GNC-CASH-TYPE 1) (define GNC-ASSET-TYPE 2) (define GNC-CCARD-TYPE 3) (define GNC-LIABILITY-TYPE 4)
(define (gnc:string-replace-char s old new)
  (string-map (lambda (c) (if (char=? c old) new c)) s))
(define (gnc:string-delete-chars s chars)
  (string-delete (lambda (c) (string-index chars c)) s))
