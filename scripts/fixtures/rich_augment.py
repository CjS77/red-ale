"""Hand-add every KVP type, a budget, template transactions and an SX to rich.xml -> aug.xml.

Deliberately non-canonical input (unsorted keys, odd formatting, offsets) so the oracle's
load+save shows how GnuCash normalises it."""
import sys

src = open(sys.argv[1], encoding="utf-8").read()
CHK = "c080059e1aec4c0b9ad4671e80965686"
FOOD = "1feca2abf92347288c268ebf93753ec4"
CASH = "9ea336dec8b64310a48810eee3325a51"
SXID = "52f96837087e9670c933f9421b0ac626"
TROOT = "4586cdb4267a59437526549e14b0e74b"
TACCT = "22f2905cbcfa0b910c0dbc348cfc1161"

slots = """    <slot>
      <slot:key>zz-list</slot:key>
      <slot:value type="list">
        <slot:value type="integer">3</slot:value>
        <slot:value type="string">two</slot:value>
        <slot:value type="frame"><slot><slot:key>k</slot:key><slot:value type="integer">1</slot:value></slot></slot:value>
        <slot:value type="list"/>
      </slot:value>
    </slot>
    <slot><slot:key>aa-double</slot:key><slot:value type="double">0.1</slot:value></slot>
    <slot><slot:key>ab-double-big</slot:key><slot:value type="double">12345678901234567890</slot:value></slot>
    <slot><slot:key>ac-double-int</slot:key><slot:value type="double">3</slot:value></slot>
    <slot><slot:key>mm-gdate</slot:key><slot:value type="gdate"><gdate>2024-02-29</gdate></slot:value></slot>
    <slot><slot:key>mm-timespec</slot:key><slot:value type="timespec"><ts:date>2024-06-02 10:59:00 -0500</ts:date></slot:value></slot>
    <slot><slot:key>mm-timespec-frac</slot:key><slot:value type="timespec"><ts:date>2024-06-02 10:59:00.123 +0000</ts:date></slot:value></slot>
    <slot><slot:key>mm-timespec-smalloff</slot:key><slot:value type="timespec"><ts:date>2024-06-02 10:59:00 +0030</ts:date></slot:value></slot>
    <slot><slot:key>nn-guid</slot:key><slot:value type="guid">c080059e1aec4c0b9ad4671e80965686</slot:value></slot>
    <slot><slot:key>nn-numeric</slot:key><slot:value type="numeric">-5/3</slot:value></slot>
    <slot><slot:key>nn-numeric-nodenom</slot:key><slot:value type="numeric">7</slot:value></slot>
    <slot><slot:key>nn-numeric-decimal</slot:key><slot:value type="numeric">1.50</slot:value></slot>
    <slot><slot:key>nn-numeric-bad</slot:key><slot:value type="numeric">abc</slot:value></slot>
    <slot><slot:key>ii-integer</slot:key><slot:value type="integer">-9223372036854775808</slot:value></slot>
    <slot><slot:key>ee-emptyframe</slot:key><slot:value type="frame"/></slot>
    <slot><slot:key>es-emptystring</slot:key><slot:value type="string"/></slot>
    <slot><slot:key>es-ws</slot:key><slot:value type="string">  padded  </slot:value></slot>
    <slot><slot:key>es-charref</slot:key><slot:value type="string">&#xE9;&#233;&#x20AC;&#x9;tab&#13;&#10;&quot;&apos;</slot:value></slot>
    <slot><slot:key>es-cdata</slot:key><slot:value type="string"><![CDATA[<cdata & stuff>]]></slot:value></slot>
    <slot><slot:key>dup</slot:key><slot:value type="string">first</slot:value></slot>
    <slot><slot:key>dup</slot:key><slot:value type="string">second</slot:value></slot>
    <slot><slot:key>a/b</slot:key><slot:value type="string">slash key</slot:value></slot>
    <slot><slot:key>unknown-type</slot:key><slot:value type="binary">00ff</slot:value></slot>
    <slot><slot:key>Zupper</slot:key><slot:value type="string">upper sorts before lower</slot:value></slot>
    <slot>
      <slot:key>frame-nested</slot:key>
      <slot:value type="frame">
        <slot><slot:key>b</slot:key><slot:value type="string">B</slot:value></slot>
        <slot><slot:key>a</slot:key><slot:value type="frame"><slot><slot:key>deep</slot:key><slot:value type="numeric">1/3</slot:value></slot></slot:value></slot>
      </slot:value>
    </slot>
"""
anchor = "  <act:slots>\n    <slot>\n      <slot:key>color</slot:key>"
assert src.count(anchor) == 1
src = src.replace(anchor, "  <act:slots>\n" + slots + "    <slot>\n      <slot:key>color</slot:key>")

budget = f"""<gnc:budget version="2.0.0">
  <bgt:id type="guid">0123456789abcdef0123456789abcdef</bgt:id>
  <bgt:name>Budget 2024</bgt:name>
  <bgt:description/>
  <bgt:num-periods>12</bgt:num-periods>
  <bgt:recurrence version="1.0.0">
    <recurrence:mult>1</recurrence:mult>
    <recurrence:period_type>month</recurrence:period_type>
    <recurrence:start>
      <gdate>2024-01-01</gdate>
    </recurrence:start>
  </bgt:recurrence>
  <bgt:slots>
    <slot>
      <slot:key>{FOOD}</slot:key>
      <slot:value type="frame">
        <slot><slot:key>2</slot:key><slot:value type="numeric">300/1</slot:value></slot>
        <slot><slot:key>10</slot:key><slot:value type="numeric">1100/1</slot:value></slot>
        <slot><slot:key>0</slot:key><slot:value type="numeric">100/1</slot:value></slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>notes</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>{FOOD}</slot:key>
          <slot:value type="frame">
            <slot><slot:key>0</slot:key><slot:value type="string">January food</slot:value></slot>
          </slot:value>
        </slot>
      </slot:value>
    </slot>
  </bgt:slots>
</gnc:budget>
"""

template = f"""<gnc:template-transactions>
<gnc:account version="2.0.0">
  <act:name>Template Root</act:name>
  <act:id type="guid">{TROOT}</act:id>
  <act:type>ROOT</act:type>
  <act:commodity>
    <cmdty:space>template</cmdty:space>
    <cmdty:id>template</cmdty:id>
  </act:commodity>
  <act:commodity-scu>1</act:commodity-scu>
  <act:non-standard-scu/>
</gnc:account>
<gnc:account version="2.0.0">
  <act:name>{SXID}</act:name>
  <act:id type="guid">{TACCT}</act:id>
  <act:type>BANK</act:type>
  <act:commodity>
    <cmdty:space>template</cmdty:space>
    <cmdty:id>template</cmdty:id>
  </act:commodity>
  <act:commodity-scu>1</act:commodity-scu>
  <act:non-standard-scu/>
  <act:parent type="guid">{TROOT}</act:parent>
</gnc:account>
<gnc:transaction version="2.0.0">
  <trn:id type="guid">68bf5e7740392eb15a079ee99f18a357</trn:id>
  <trn:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>USD</cmdty:id>
  </trn:currency>
  <trn:date-posted>
    <ts:date>2024-08-01 10:59:00 +0000</ts:date>
  </trn:date-posted>
  <trn:date-entered>
    <ts:date>2024-08-01 08:40:17 +0000</ts:date>
  </trn:date-entered>
  <trn:description>Rent</trn:description>
  <trn:splits>
    <trn:split>
      <split:id type="guid">676a518eb1cf05ca552434533288a5cd</split:id>
      <split:reconciled-state>n</split:reconciled-state>
      <split:value>0/100</split:value>
      <split:quantity>0/1</split:quantity>
      <split:account type="guid">{TACCT}</split:account>
      <split:slots>
        <slot>
          <slot:key>sched-xaction</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>account</slot:key>
              <slot:value type="guid">{FOOD}</slot:value>
            </slot>
            <slot>
              <slot:key>credit-formula</slot:key>
              <slot:value type="string"></slot:value>
            </slot>
            <slot>
              <slot:key>credit-numeric</slot:key>
              <slot:value type="numeric">0/1</slot:value>
            </slot>
            <slot>
              <slot:key>debit-formula</slot:key>
              <slot:value type="string">456</slot:value>
            </slot>
            <slot>
              <slot:key>debit-numeric</slot:key>
              <slot:value type="numeric">456/1</slot:value>
            </slot>
          </slot:value>
        </slot>
      </split:slots>
    </trn:split>
    <trn:split>
      <split:id type="guid">1b779b05012759adbac315f1f50af621</split:id>
      <split:reconciled-state>n</split:reconciled-state>
      <split:value>0/100</split:value>
      <split:quantity>0/1</split:quantity>
      <split:account type="guid">{TACCT}</split:account>
      <split:slots>
        <slot>
          <slot:key>sched-xaction</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>account</slot:key>
              <slot:value type="guid">{CHK}</slot:value>
            </slot>
            <slot>
              <slot:key>credit-formula</slot:key>
              <slot:value type="string">456</slot:value>
            </slot>
            <slot>
              <slot:key>credit-numeric</slot:key>
              <slot:value type="numeric">456/1</slot:value>
            </slot>
            <slot>
              <slot:key>debit-formula</slot:key>
              <slot:value type="string"></slot:value>
            </slot>
            <slot>
              <slot:key>debit-numeric</slot:key>
              <slot:value type="numeric">0/1</slot:value>
            </slot>
          </slot:value>
        </slot>
      </split:slots>
    </trn:split>
  </trn:splits>
</gnc:transaction>
</gnc:template-transactions>
<gnc:schedxaction version="2.0.0">
  <sx:id type="guid">{SXID}</sx:id>
  <sx:name>Monthly rent</sx:name>
  <sx:enabled>y</sx:enabled>
  <sx:autoCreate>n</sx:autoCreate>
  <sx:autoCreateNotify>n</sx:autoCreateNotify>
  <sx:advanceCreateDays>0</sx:advanceCreateDays>
  <sx:advanceRemindDays>5</sx:advanceRemindDays>
  <sx:instanceCount>2</sx:instanceCount>
  <sx:start>
    <gdate>2024-08-01</gdate>
  </sx:start>
  <sx:last>
    <gdate>2024-09-01</gdate>
  </sx:last>
  <sx:end>
    <gdate>2025-07-01</gdate>
  </sx:end>
  <sx:templ-acct type="guid">{TACCT}</sx:templ-acct>
  <sx:schedule>
    <gnc:recurrence version="1.0.0">
      <recurrence:mult>1</recurrence:mult>
      <recurrence:period_type>month</recurrence:period_type>
      <recurrence:start>
        <gdate>2024-08-01</gdate>
      </recurrence:start>
      <recurrence:weekend_adj>back</recurrence:weekend_adj>
    </gnc:recurrence>
  </sx:schedule>
  <sx:deferredInstance>
    <sx:last>
      <gdate>2024-09-01</gdate>
    </sx:last>
    <sx:rem-occur>9</sx:rem-occur>
    <sx:instanceCount>3</sx:instanceCount>
  </sx:deferredInstance>
</gnc:schedxaction>
"""
# GnuCash order: transactions, template-transactions, schedxactions, budgets.
# Put the budget FIRST and the SX block after it to show GnuCash reorders on save.
end = "</gnc:book>\n"
src = src.replace(end, budget + template + end)
open(sys.argv[2], "w", encoding="utf-8").write(src)
