<?xml version="1.0" encoding="UTF-8"?>
<!-- Nulis sample: XSL -->
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:output method="html" indent="yes"/>

  <!-- Template 0 -->
  <xsl:template match="book[@id='bk000']">
    <div class="book alpha">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 0">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 1 -->
  <xsl:template match="book[@id='bk001']">
    <div class="book beta">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 1">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 2 -->
  <xsl:template match="book[@id='bk002']">
    <div class="book gamma">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 2">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 3 -->
  <xsl:template match="book[@id='bk003']">
    <div class="book delta">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 3">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 4 -->
  <xsl:template match="book[@id='bk004']">
    <div class="book epsilon">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 4">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 5 -->
  <xsl:template match="book[@id='bk005']">
    <div class="book zeta">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 5">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 6 -->
  <xsl:template match="book[@id='bk006']">
    <div class="book eta">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 6">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 7 -->
  <xsl:template match="book[@id='bk007']">
    <div class="book theta">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 7">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 8 -->
  <xsl:template match="book[@id='bk008']">
    <div class="book iota">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 8">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Template 9 -->
  <xsl:template match="book[@id='bk009']">
    <div class="book kappa">
      <h2><xsl:value-of select="title"/></h2>
      <xsl:if test="price &gt; 9">
        <p class="expensive">Price: <xsl:value-of select="format-number(price, '0.00')"/></p>
      </xsl:if>
    </div>
  </xsl:template>
</xsl:stylesheet>
