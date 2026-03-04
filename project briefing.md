Project Briefing: AI-Native XFA Protocol Engineering (Rust Edition)
===================================================================

1\. Doelstelling
----------------

Het bouwen van een high-performance, visueel accurate XFA (XML Forms Architecture) engine in **Rust**. De engine moet de 1500+ pagina's aan officiële specificaties implementeren om volledige parity met Adobe Reader te bereiken, inclusief dynamic reflow en FormCalc-scripting.

2\. De Bron van Waarheid
------------------------

Claude Code moet onderstaande documenten gebruiken als de enige bron voor architecturale beslissingen:

*   **XFA 3.3 Kernspecificatie:** [https://pdfa.org/norm-refs/XFA-3\_3.pdf](https://pdfa.org/norm-refs/XFA-3_3.pdf)
    
*   **FormCalc Taalreferentie:** [https://helpx.adobe.com/pdf/aem-forms/6-2/formcalc-reference.pdf](https://helpx.adobe.com/pdf/aem-forms/6-2/formcalc-reference.pdf)
    

3\. Technische Stack & Motivatie
--------------------------------

*   **Taal:** Rust (v1.75+).
    
*   **XML Parsing:** roxmltree (voor een snelle, read-only view op de Template/Data DOM).
    
*   **PDF Manipulatie:** pdfium-render (Rust bindings voor PDFium) om de XFA-engine te koppelen aan de visuele PDF-laag.
    
*   **Scripting:** Een aangepaste interpreter die FormCalc AST (Abstract Syntax Tree) omzet in veilige Rust-executie of JavaScript-interop.
    

4\. Modulaire Implementatiestrategie
------------------------------------

### Module A: xfa-dom-resolver

*   **Taal:** Rust.
    
*   **Taak:** Implementeer de hiërarchische Scripting Object Model (SOM) paden uit Sectie 3 van de spec .
    
*   **AI-Focus:** De AI moet algoritmes bouwen die paden zoals xfa.form.subform.\[3\]field\[\*\] razendsnel kunnen resolven in zowel de Template- als de Data-DOM .
    

### Module B: xfa-layout-engine (De Core)

*   **Taal:** Rust.
    
*   **Taak:** Implementatie van het XFA Box Model (Hoofdstuk 4) .
    
*   **Logica:** Berekenen van flowed vs. positioned containers. Afhandelen van minH, maxH en herhalende subforms (occur rules) .
    
*   **AI-Focus:** Gebruik de AI om de wiskundige definities van pagination en content overflow exact om te zetten in code die de paginalengte dynamisch aanpast.
    

### Module C: formcalc-rust-interpreter

*   **Taal:** Rust.
    
*   **Taak:** Een volledige implementatie van de FormCalc grammatica .
    
*   **AI-Focus:** Gebruik Claude om een parser te genereren die alle built-in functies (Sum, Avg, Date2Num) implementeert volgens de exacte Adobe-definities.
    

### Module D: pdfium-ffi-bridge

*   **Taal:** Rust (unsafe bridge).
    
*   **Taak:** Implementeren van de FPDF\_FORMFILLINFO callbacks.
    
*   **AI-Focus:** Koppelen van de Rust XFA-engine aan de C++ PDFium-runtime via FFI, zodat UI-events (zoals muisklikken) direct de Rust-engine triggeren .
    

5\. Visual Testing & Validatie
------------------------------

Echte compatibiliteit vereist visuele bewijslast:

1.  **Golden Render Pipeline:** Claude moet een script schrijven dat een PDF rendert en pixel-voor-pixel vergelijkt met een screenshot van Adobe Reader.
    
2.  **AI-Vision Debugging:** Bij afwijkingen moet Claude Code vision-modellen gebruiken om te analyseren _waarom_ de layout verschilt (bijv. "de padding wordt niet correct toegepast op Module B") .
    

6\. Save & Persistence Strategie
--------------------------------

*   **Datasets Sync:** De engine moet bij elke save-actie de packet in de PDF overschrijven met de actuele status van de Rust-DOM.
    
*   **Usage Rights Mitigation:** De AI moet logica bevatten die detecteert of UR3 signatures aanwezig zijn en deze desgewenst veilig verwijdert om Adobe Reader-fouten te voorkomen.
    

Start-opdracht voor Claude Code:
--------------------------------

> "Initialiseer project 'XFA-Native-Rust'.
> 
> 1.  Gebruik [https://pdfa.org/norm-refs/XFA-3\_3.pdf](https://pdfa.org/norm-refs/XFA-3_3.pdf) als primaire technische bron.
>     
> 2.  Stel een SPEC.md op in Plan Mode die de Rust-architectuur beschrijft voor de Layout Engine, gebaseerd op het Box Model in Hoofdstuk 4.
>     
> 3.  Gebruik de roxmltree crate voor XML-verwerking.
>     
> 4.  Definieer een testsuite die 'Golden Renders' gebruikt om visuele correctheid te garanderen."
>