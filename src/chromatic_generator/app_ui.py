#This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

#This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

#You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

###########################################################################
## Python code generated with wxFormBuilder (version Oct 26 2018)
## http://www.wxformbuilder.org/
##
## PLEASE DO *NOT* EDIT THIS FILE!
###########################################################################

import wx
import wx.xrc

###########################################################################
## Class AppFrame
###########################################################################

class AppFrame ( wx.Frame ):

	def __init__( self, parent ):
		wx.Frame.__init__ ( self, parent, id = wx.ID_ANY, title = wx.EmptyString, pos = wx.DefaultPosition, size = wx.Size( 600,500 ), style = wx.DEFAULT_FRAME_STYLE|wx.TAB_TRAVERSAL )

		self.SetSizeHints( wx.Size( 600,500 ), wx.Size( 600,500 ) )
		self.SetBackgroundColour( wx.Colour( 248, 248, 248 ) )

		mainSizer = wx.BoxSizer( wx.VERTICAL )

		titleSizer = wx.BoxSizer( wx.HORIZONTAL )


		titleSizer.Add( ( 0, 0), 1, wx.EXPAND, 5 )

		self.titleLabel = wx.StaticText( self, wx.ID_ANY, u"Chromatic Scale Generator", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.titleLabel.Wrap( -1 )

		self.titleLabel.SetFont( wx.Font( 25, wx.FONTFAMILY_DEFAULT, wx.FONTSTYLE_NORMAL, wx.FONTWEIGHT_BOLD, False, wx.EmptyString ) )

		titleSizer.Add( self.titleLabel, 0, wx.ALIGN_CENTER_VERTICAL, 0 )


		titleSizer.Add( ( 0, 0), 1, wx.EXPAND, 5 )


		mainSizer.Add( titleSizer, 0, wx.BOTTOM|wx.EXPAND|wx.TOP, 10 )

		dirSizer = wx.BoxSizer( wx.HORIZONTAL )

		self.dirText = wx.StaticText( self, wx.ID_ANY, u"Sample folder:", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.dirText.Wrap( -1 )

		dirSizer.Add( self.dirText, 0, wx.ALIGN_CENTER|wx.ALL, 5 )

		self.dirPicker = wx.DirPickerCtrl( self, wx.ID_ANY, wx.EmptyString, u"Select a folder", wx.DefaultPosition, wx.DefaultSize, wx.DIRP_DEFAULT_STYLE )
		dirSizer.Add( self.dirPicker, 0, wx.ALIGN_CENTER|wx.ALL, 5 )


		mainSizer.Add( dirSizer, 0, wx.LEFT, 5 )

		startNoteSizer = wx.BoxSizer( wx.HORIZONTAL )

		self.startNoteText = wx.StaticText( self, wx.ID_ANY, u"Starting note:", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.startNoteText.Wrap( -1 )

		startNoteSizer.Add( self.startNoteText, 0, wx.ALIGN_CENTER|wx.ALL, 5 )

		startNoteChoiceChoices = [ u"C", u"C#", u"D", u"D#", u"E", u"F", u"F#", u"G", u"G#", u"A", u"A#", u"B" ]
		self.startNoteChoice = wx.Choice( self, wx.ID_ANY, wx.DefaultPosition, wx.DefaultSize, startNoteChoiceChoices, 0 )
		self.startNoteChoice.SetSelection( 0 )
		startNoteSizer.Add( self.startNoteChoice, 0, wx.ALIGN_CENTER|wx.ALL, 5 )


		mainSizer.Add( startNoteSizer, 0, wx.LEFT, 5 )

		startOctaveSizer = wx.BoxSizer( wx.HORIZONTAL )

		self.startOctaveText = wx.StaticText( self, wx.ID_ANY, u"Starting octave:", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.startOctaveText.Wrap( -1 )

		startOctaveSizer.Add( self.startOctaveText, 0, wx.ALIGN_CENTER|wx.ALL, 5 )

		startOctaveChoiceChoices = [ u"2", u"3", u"4" ]
		self.startOctaveChoice = wx.Choice( self, wx.ID_ANY, wx.DefaultPosition, wx.DefaultSize, startOctaveChoiceChoices, 0 )
		self.startOctaveChoice.SetSelection( 0 )
		startOctaveSizer.Add( self.startOctaveChoice, 0, wx.ALIGN_CENTER|wx.ALL, 5 )


		mainSizer.Add( startOctaveSizer, 0, wx.LEFT, 5 )

		rangeSizer = wx.BoxSizer( wx.HORIZONTAL )

		self.rangeText = wx.StaticText( self, wx.ID_ANY, u"Range:", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.rangeText.Wrap( -1 )

		rangeSizer.Add( self.rangeText, 0, wx.ALIGN_CENTER|wx.ALL, 5 )

		self.rangeInput = wx.TextCtrl( self, wx.ID_ANY, u"24", wx.DefaultPosition, wx.DefaultSize, 0 )
		rangeSizer.Add( self.rangeInput, 0, wx.ALL, 5 )


		mainSizer.Add( rangeSizer, 0, wx.LEFT, 5 )

		gapSizer = wx.BoxSizer( wx.HORIZONTAL )

		self.gapText = wx.StaticText( self, wx.ID_ANY, u"Sample gap (seconds):", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.gapText.Wrap( -1 )

		gapSizer.Add( self.gapText, 0, wx.ALIGN_CENTER|wx.ALL, 5 )

		self.gapInput = wx.TextCtrl( self, wx.ID_ANY, u"0.1", wx.DefaultPosition, wx.DefaultSize, 0 )
		gapSizer.Add( self.gapInput, 0, wx.ALL, 5 )


		mainSizer.Add( gapSizer, 0, wx.LEFT, 5 )

		self.pitchedCheck = wx.CheckBox( self, wx.ID_ANY, u"Pitched", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.pitchedCheck.SetValue(True)
		mainSizer.Add( self.pitchedCheck, 0, wx.ALL, 10 )

		self.samplesCheck = wx.CheckBox( self, wx.ID_ANY, u"Dump individual pitched samples", wx.DefaultPosition, wx.DefaultSize, 0 )
		self.samplesCheck.SetValue(True)
		mainSizer.Add( self.samplesCheck, 0, wx.ALL|wx.LEFT, 10 )

		self.chromGenerate = wx.Button( self, wx.ID_ANY, u"Generate Chromatic", wx.DefaultPosition, wx.DefaultSize, 0 )
		mainSizer.Add( self.chromGenerate, 0, wx.ALL, 10 )


		self.SetSizer( mainSizer )
		self.Layout()

		self.Centre( wx.BOTH )

		# Connect Events
		self.chromGenerate.Bind( wx.EVT_BUTTON, self.generate_chromatic )

	def __del__( self ):
		pass


	# Virtual event handlers, overide them in your derived class
	def generate_chromatic( self, event ):
		event.Skip()


